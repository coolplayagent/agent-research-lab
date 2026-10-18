//! Retained process cancellation and abandoned-attempt reconciliation.
use crate::completion::{ExitObservation, settle};
use crate::job_store::{job_path, jobs};
use crate::jobs::defer_read_retry;
use crate::post_success::queue_post_success;
use crate::team_channel::revoke_communication;
use crate::worker_identity::process_start;
use anyhow::{Context, Result};
use config::Config;
use contracts::WorkflowState;
use serde_json::Value;
use workflow::Workflow;

/// Safety cancellation must also run when pause or budget prevents any CLI
/// recovery. Only saved process groups with matching Linux start time qualify.
pub(super) fn kill_retained_workers(c: &Config) -> Result<()> {
    for job in jobs(c)? {
        if let Some(launch) = job.launch
            && launch.pid > 1
            && process_start(launch.pid).is_ok_and(|s| s == launch.process_start)
        {
            let result = unsafe { libc::kill(-(launch.pid as i32), libc::SIGKILL) };
            if result != 0 {
                let error = std::io::Error::last_os_error();
                if error.raw_os_error() != Some(libc::ESRCH) {
                    return Err(error).context("cancel retained worker process group");
                }
            }
        }
    }
    Ok(())
}

pub(super) fn reconcile_orphans(c: &Config, w: &Workflow) -> Result<()> {
    for mut job in jobs(c)? {
        if let Some(launch) = job.launch.as_mut() {
            lease_heartbeat::restore(w, &mut launch.lease)?;
            storage::write(&job_path(c, &job.task.id), &job)?;
        }
        if let Some(launch) = job.launch.clone() {
            if launch.pid > 0 && process_start(launch.pid).is_ok_and(|s| s == launch.process_start)
            {
                // Retained PID + Linux start time prevents signalling a recycled process.
                unsafe {
                    libc::kill(-(launch.pid as i32), libc::SIGKILL);
                }
            }
            let state = w.status(&job.run_id)?;
            if [
                Some(WorkflowState::Succeeded),
                Some(WorkflowState::Failed),
                Some(WorkflowState::Cancelled),
            ]
            .contains(&WorkflowState::from_value(&state["status"]))
            {
                let _ = w.release(&launch.lease);
                job.launch = None;
                if WorkflowState::from_value(&state["status"]) == Some(WorkflowState::Succeeded) {
                    job.last_error = None;
                    job.retry_after = None;
                } else {
                    defer_read_retry(
                        &mut job,
                        "retained terminal worker failure recovered".into(),
                    );
                }
            } else {
                let observation: ExitObservation = storage::read(
                    &w.work_dir
                        .join("host-exits")
                        .join(format!("{}.json", job.run_id)),
                )
                .unwrap_or(ExitObservation {
                    success: false,
                    code: None,
                    reason: Some("controller interrupted; process outcome unavailable".into()),
                });
                if launch.lease["expires_at_unix_ms"].as_u64().unwrap_or(0) > workflow::now_ms()?
                    && settle(w, &job, &observation).is_ok()
                {
                    let _ = w.release(&launch.lease);
                    job.launch = None;
                    if WorkflowState::from_value(&w.status(&job.run_id)?["status"])
                        == Some(WorkflowState::Succeeded)
                    {
                        job.last_error = None;
                        job.retry_after = None;
                    } else {
                        defer_read_retry(
                            &mut job,
                            "interrupted worker settled; see retained observations".into(),
                        );
                    }
                } else {
                    // A newer lease cannot fabricate authority for an old attempt.
                    // Pause the abandoned run. Read-only retries use a new bounded run;
                    // writes retain the unresolved intent and require explicit reconciliation.
                    if state.get("pause").is_none_or(Value::is_null) {
                        w.pause(
                            &job.run_id,
                            &format!("recover-{}", workflow::now_ms()?),
                            state["revision"]
                                .as_u64()
                                .context("missing workflow revision")?,
                            workflow::now_ms()?,
                            "controller interrupted; previous attempt fenced",
                        )?;
                    }
                    job.launch = None;
                    defer_read_retry(&mut job,"controller interrupted; old run paused, inspect workflow effects before write retry".into());
                }
            }
            if WorkflowState::from_value(&w.status(&job.run_id)?["status"])
                != Some(WorkflowState::Succeeded)
            {
                revoke_communication(c, &job, &launch);
            }
            storage::write(&job_path(c, &job.task.id), &job)?;
        }
        // Include terminal jobs whose launch was already cleared when the
        // controller stopped between durable success and queue publication.
        if job.launch.is_none()
            && WorkflowState::from_value(&w.status(&job.run_id)?["status"])
                == Some(WorkflowState::Succeeded)
        {
            queue_post_success(c, w, &job)?;
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "recovery_tests.rs"]
mod tests;
