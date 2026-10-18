//! Own active process guards and settle only their retained attempt authority.
use super::eligibility::may_join;
use crate::{
    admission,
    completion::{ExitObservation, settle_with_heartbeat},
    jobs::defer_read_retry,
    post_success::queue_post_success,
    team_channel::revoke_communication,
};
use anyhow::{Context, Result};
use config::Config;
use process::Process;
use serde_json::json;
use task::Job;
use workflow::Workflow;

struct Active {
    job: Job,
    job_writes: admission::JobWrites,
    process: Process,
    heartbeat: lease_heartbeat::LeaseHeartbeat,
}

#[derive(Default)]
pub(super) struct Workers {
    active: Vec<Active>,
    next_communication_poll: i64,
}

impl Workers {
    pub(super) fn is_empty(&self) -> bool {
        self.active.is_empty()
    }

    pub(super) fn len(&self) -> usize {
        self.active.len()
    }

    pub(super) fn task_ids(&self) -> Vec<String> {
        self.active
            .iter()
            .map(|worker| worker.job.task.id.clone())
            .collect()
    }

    pub(super) fn exploratory(&self) -> Option<bool> {
        self.active
            .first()
            .map(|worker| worker.job.task.exploratory)
    }

    pub(super) fn may_admit(&self, job: &Job) -> bool {
        may_join(self.active.first().map(|worker| &worker.job), job)
    }

    pub(super) fn push(
        &mut self,
        job: Job,
        job_writes: admission::JobWrites,
        process: Process,
        heartbeat: lease_heartbeat::LeaseHeartbeat,
    ) {
        self.active.push(Active {
            job,
            job_writes,
            process,
            heartbeat,
        });
    }

    /// Return whether guarded admission must remain stopped after this poll.
    /// The caller latches that result; worker polling never clears a prior failure.
    pub(super) fn poll(
        &mut self,
        c: &Config,
        w: &Workflow,
        stopping: bool,
        time: i64,
        admission_hook: &mut Option<&mut dyn admission::ScopedAdmissionHook>,
    ) -> Result<bool> {
        let mut guarded_admission_failed = false;
        if !stopping && !self.active.is_empty() && time >= self.next_communication_poll {
            let members = self
                .active
                .iter()
                .filter_map(|a| {
                    a.job
                        .launch
                        .as_ref()?
                        .communication
                        .as_ref()
                        .map(|v| v.member.clone())
                })
                .collect::<Vec<_>>();
            if !members.is_empty() {
                let report =
                    match multi_agent::poll_active(&c.state_dir, &members, time.max(0) as u64) {
                        Ok(report) => report,
                        Err(error) => json!({"error":error.to_string()}),
                    };
                let _ = storage::write(
                    &c.state_dir.join("communication/poll-status.json"),
                    &json!({"at":time,"result":report}),
                );
            }
            self.next_communication_poll = time + 2;
        }
        let mut i = 0;
        while i < self.active.len() {
            let mut polled = if stopping {
                self.active[i].process.cancel().map(|_| None)
            } else {
                self.active[i].process.poll()
            };
            if let Some(error) = self.active[i].heartbeat.failure() {
                let _ = self.active[i].process.cancel();
                polled = Err(anyhow::anyhow!("lease heartbeat failed: {error}"));
            }
            if stopping || !matches!(polled, Ok(None)) {
                let _cleanup = process::cleanup_scope();
                let mut a = self.active.remove(i);
                a.heartbeat.disarm_worker();
                if let Err(drift) = a.job_writes.confirm(c) {
                    if let Some(hook) = admission_hook.as_deref_mut() {
                        hook.revoke(&format!("guarded active Job changed: {drift:#}"));
                    }
                    let lease = a.heartbeat.stop()?;
                    let _ = w.release(&lease);
                    return Err(drift);
                }
                let observation = ExitObservation {
                    success: matches!(&polled,Ok(Some(status)) if status.success()) && !stopping,
                    code: match &polled {
                        Ok(Some(status)) => status.code(),
                        _ => None,
                    },
                    reason: if stopping {
                        Some("controller budget/pause stop".into())
                    } else {
                        polled.as_ref().err().map(ToString::to_string)
                    },
                };
                a.job
                    .launch
                    .as_mut()
                    .context("active job lacks launch")?
                    .lease = a.heartbeat.current()?;
                let mut launch = a
                    .job
                    .launch
                    .as_ref()
                    .context("active job lacks launch")?
                    .clone();
                storage::write(
                    &w.work_dir
                        .join("host-exits")
                        .join(format!("{}.json", a.job.run_id)),
                    &observation,
                )?;
                match settle_with_heartbeat(w, &a.job, &observation, Some(&a.heartbeat)) {
                    Ok(settled_success) => {
                        if !settled_success || launch.communication_gap.is_some() {
                            revoke_communication(c, &a.job, &launch);
                        }
                        a.job.launch = None;
                        if settled_success {
                            a.job.last_error = None;
                            a.job.retry_after = None;
                            queue_post_success(c, w, &a.job)?;
                        } else {
                            let reason = observation
                                .reason
                                .clone()
                                .unwrap_or_else(|| format!("worker exited {:?}", observation.code));
                            if let Some(hook) = admission_hook.as_deref_mut() {
                                hook.revoke(&format!("guarded worker failed: {reason}"));
                                guarded_admission_failed = true;
                                a.job.last_error = Some(reason);
                                a.job.retry_after = None;
                            } else {
                                defer_read_retry(&mut a.job, reason);
                            }
                        }
                    }
                    Err(error) => {
                        revoke_communication(c, &a.job, &launch);
                        // Keep the claim for startup reconciliation after ambiguous settlement.
                        a.job.last_error =
                            Some(format!("settlement requires reconciliation: {error:#}"));
                        a.job.retry_after = None;
                    }
                }
                launch.lease = a.heartbeat.stop()?;
                if let Some(retained) = a.job.launch.as_mut() {
                    retained.lease = launch.lease.clone();
                }
                let released = w.release(&launch.lease);
                if let Err(error) = released {
                    if a.job.launch.is_some() || admission_hook.is_some() {
                        a.job.last_error = Some(format!(
                            "settlement/lease reconciliation required: {error:#}"
                        ));
                    } else {
                        storage::write(
                            &launch.log_dir.join("release-error.json"),
                            &json!({"error":error.to_string()}),
                        )?;
                    }
                }
                if admission_hook.is_some() && a.job.last_error.is_some() {
                    a.job.retry_after = None;
                    guarded_admission_failed = true;
                    if let Some(hook) = admission_hook.as_deref_mut() {
                        hook.revoke("guarded settlement requires reconciliation");
                    }
                }
                a.job_writes.persist(c, &a.job)?;
                continue;
            }
            i += 1;
        }
        Ok(guarded_admission_failed)
    }
}

#[cfg(test)]
#[path = "workers_tests.rs"]
mod tests;
