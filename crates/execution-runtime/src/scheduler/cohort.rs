//! Prepare one bounded cohort before its first lease, retaining prerequisite backoff.
use super::eligibility::{admission_ready, may_join};
use crate::{
    admission,
    host_budget::{Ledger, RunWindow, now, prepare_prerequisite},
    jobs::prepare_retry,
};
use anyhow::{Context, Result, ensure};
use config::Config;
use contracts::ExecutionState;
use native_effects::fs;
use serde_json::json;
use std::{collections::BTreeMap, time::Duration};
use task::Job;
use workflow::Workflow;

#[derive(Default)]
pub(super) struct Prerequisites {
    desktop_prepared: bool,
    next_desktop_prepare: i64,
    next_build_prepare: BTreeMap<String, i64>,
}

/// Borrow the controller's accounting state; preparation cannot restart its window.
pub(super) struct Preparation<'a> {
    pub config: &'a Config,
    pub workflow: &'a Workflow,
    pub ledger: &'a mut Ledger,
    pub window: RunWindow,
    pub time: i64,
    pub last: &'a mut i64,
    pub guarded: bool,
}

pub(super) struct PreparedCohort {
    pub jobs: Vec<Job>,
    pub retry_waiting: bool,
    pub deferred_admission: bool,
}

impl Prerequisites {
    pub(super) fn prepare(
        &mut self,
        pending: Vec<Job>,
        may_admit: bool,
        context: Preparation<'_>,
    ) -> Result<PreparedCohort> {
        let Preparation {
            config: c,
            workflow: w,
            ledger,
            window,
            time,
            last,
            guarded,
        } = context;
        let mut retry_waiting = false;
        // Finish the selected cohort's prerequisites before the first lease.
        // A failed prerequisite takes no capacity; later ready peers can join.
        let mut prepared: Vec<Job> = Vec::new();
        let mut deferred_admission = false;
        let mut checked_cohort: Option<String> = None;
        for mut job in pending {
            let mut pending_writes = admission::JobWrites::new(&job, guarded);
            if !may_admit {
                break;
            }
            if prepared.len() >= c.max_agents {
                deferred_admission = true;
                break;
            }
            if window.remaining(c, ledger) == 0 {
                break;
            }
            if job.launch.is_some() {
                continue;
            }
            // Sandbox verification stays in launch, with no earlier peer.
            // Do not reorder it ahead of an already selected ordinary task.
            if !may_join(prepared.first(), &job) {
                deferred_admission = true;
                continue;
            }
            if let Some(due) = job.retry_after {
                ensure!(
                    !guarded,
                    "guarded admission cannot automatically retry an earlier attempt"
                );
                retry_waiting = true;
                if due > time {
                    continue;
                }
                if prepare_prerequisite(c, ledger, window, false, || prepare_retry(c, &mut job))?
                    .is_none()
                {
                    break;
                }
                *last = now();
            }
            if job.last_error.is_some() {
                continue;
            }
            let eligible =
                prepare_prerequisite(c, ledger, window, false, || admission_ready(c, w, &job))?;
            *last = now();
            match eligible {
                Some(true) => {}
                Some(false) => continue,
                None => break,
            }
            let cohort = job
                .research_inputs
                .as_ref()
                .map(inputs::cohort)
                .unwrap_or_default();
            if let Some(checked) = &checked_cohort {
                if checked != &cohort {
                    deferred_admission = true;
                    continue;
                }
            } else {
                let checked = prepare_prerequisite(c, ledger, window, false, || {
                    let _deadline = process::deadline_scope(Duration::from_secs(60));
                    inputs::verify(c, job.research_inputs.as_ref())
                });
                *last = now();
                if matches!(checked, Ok(None)) {
                    break;
                }
                if let Err(error) = checked {
                    job.last_error = Some(format!(
                        "latest-baseline admission blocked; preserve this experiment and enqueue a fresh cohort: {error:#}"
                    ));
                    job.retry_after = None;
                    pending_writes.persist(c, &job)?;
                    continue;
                }
                checked_cohort = Some(cohort);
            }
            if !self.desktop_prepared
                && job
                    .task
                    .required_tools
                    .iter()
                    .any(|name| name == "computer-use-cli")
            {
                // Cold preparation may download dependencies for up to 55 s.
                // Never run it under the 10 s allowance of an active cohort.
                retry_waiting = true;
                if now() < self.next_desktop_prepare {
                    continue;
                }
                let prepared =
                    prepare_prerequisite(c, ledger, window, false, || targets::prepare(c));
                *last = now();
                let diagnostic = c.state_dir.join("target-preparation-error.json");
                match prepared {
                    Ok(Some(_)) => {
                        self.desktop_prepared = true;
                        if diagnostic.exists() {
                            fs::remove_file(&diagnostic)?;
                        }
                    }
                    Ok(None) => break,
                    Err(error) => {
                        self.next_desktop_prepare = now() + 30;
                        storage::write(
                            &diagnostic,
                            &json!({"at":now(),"state":ExecutionState::WaitingTarget,
                                "retry_after":self.next_desktop_prepare,"error":format!("{error:#}")}),
                        )?;
                        // No workflow attempt has been claimed. The next idle
                        // interval may safely resume dependency preparation.
                        continue;
                    }
                }
            }
            let build_logs = c.state_dir.join("runs").join(&job.run_id);
            if (job.task.write
                || job
                    .task
                    .required_tools
                    .iter()
                    .any(|name| name == "qualitygate-cli"))
                && build_cache::applicable(&job.worktree)
                && !build_cache::ready(&job.worktree, &build_logs).unwrap_or(false)
            {
                // Dependency copies can be large. Prepare only in an idle,
                // accounted host window, before acquiring/claiming any effect.
                retry_waiting = true;
                if self
                    .next_build_prepare
                    .get(&job.run_id)
                    .is_some_and(|due| *due > now())
                {
                    continue;
                }
                let prepared = prepare_prerequisite(c, ledger, window, false, || {
                    build_cache::prepare(&job.worktree, &build_logs)
                });
                *last = now();
                match prepared {
                    Ok(Some(_)) => {
                        self.next_build_prepare.remove(&job.run_id);
                    }
                    Ok(None) => break,
                    Err(error) if build_cache::deferred(&error) => {
                        self.next_build_prepare
                            .insert(job.run_id.clone(), now() + 30);
                        continue;
                    }
                    Err(error) => {
                        job.last_error = Some(format!(
                            "build cache preparation failed before claim: {error:#}"
                        ));
                        pending_writes.persist(c, &job)?;
                        continue;
                    }
                }
            }
            let availability = prepare_prerequisite(c, ledger, window, false, || {
                // DesktopSession verifies the cached dependency manifest in
                // each worker. Availability probing never downloads packages.
                job.task.required_tools.iter().try_for_each(|name| {
                    let t = c
                        .tools
                        .get(name)
                        .context("required tool no longer configured")?;
                    let out = process::capture(
                        t.binary.to_str().context("non-UTF8 tool path")?,
                        &t.probe,
                        &c.workspace,
                        Duration::from_secs(15),
                    )?;
                    ensure!(out.status.success(), "required tool {name} unavailable");
                    Ok::<_, anyhow::Error>(())
                })
            });
            *last = now();
            if matches!(availability, Ok(None)) {
                break;
            }
            if let Err(error) = availability {
                // Missing bindings block this task until explicit retry/config repair.
                job.last_error = Some(error.to_string());
                job.retry_after = None;
                pending_writes.persist(c, &job)?;
                continue;
            }
            prepared.push(job);
        }
        // Preparation and probes can be slow even when caches are warm. Check
        // latest inputs again while still lease-free, then retain local launch
        // checks for every task's frozen bytes, prompt, config and dependencies.
        if let Some(first) = prepared.first() {
            let checked = prepare_prerequisite(c, ledger, window, false, || {
                inputs::verify(c, first.research_inputs.as_ref())
            });
            *last = now();
            match checked {
                Ok(Some(())) => {}
                Ok(None) => prepared.clear(),
                Err(error) => {
                    for job in &mut prepared {
                        let mut writes = admission::JobWrites::new(job, guarded);
                        job.last_error = Some(format!(
                            "latest-baseline admission blocked; preserve this experiment and enqueue a fresh cohort: {error:#}"
                        ));
                        job.retry_after = None;
                        writes.persist(c, job)?;
                    }
                    prepared.clear();
                }
            }
        }
        Ok(PreparedCohort {
            jobs: prepared,
            retry_waiting,
            deferred_admission,
        })
    }
}

#[cfg(test)]
#[path = "cohort_tests.rs"]
mod tests;
