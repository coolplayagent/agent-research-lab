//! Same-controller, opt-in admission. Live proof permits belong to the host hook.
use anyhow::{Result, ensure};
use config::Config;
use contracts::AcquirePreconditionsV1;
use std::path::Path;
use task::Job;

pub trait ScopedAdmissionHook {
    /// Called with controller.lock held, before watch or any native control call.
    fn register_cleanup_domain(
        &mut self,
        config: &Config,
        task_ids_file: &Path,
        max_seconds: u64,
    ) -> Result<()>;
    /// The same controller calls this after preparation/final input freshness.
    fn prepare(&mut self, config: &Config, prepared: &[Job]) -> Result<()>;
    /// Consume that lane's private current-observation permit exactly once.
    fn take_acquire_preconditions(
        &mut self,
        config: &Config,
        job: &Job,
    ) -> Result<AcquirePreconditionsV1>;
    /// Check the current Job and this attempt's persisted owned Launch authority.
    fn before_model_spawn(&mut self, config: &Config, job: &Job) -> Result<()>;
    /// All remaining permits are revoked after any admission/launch failure.
    fn revoke(&mut self, reason: &str);
    fn should_stop(&self) -> bool;
}

pub(crate) fn current_job(config: &Config, expected: &Job) -> Result<()> {
    let actual: Job = storage::read(&super::job_path(config, &expected.task.id))?;
    ensure!(
        serde_json::to_value(&actual)? == serde_json::to_value(expected)?,
        "current task attempt or frozen job authority changed"
    );
    Ok(())
}

/// Last confirmed complete Job, separate from the locally edited launch value.
/// This detects drift across host calls. The storage read/rename pair is not a
/// filesystem CAS and does not exclude an uncooperative concurrent writer.
pub(crate) struct JobWrites {
    expected: Option<Job>,
}

impl JobWrites {
    pub(crate) fn new(job: &Job, guarded: bool) -> Self {
        Self {
            expected: guarded.then(|| job.clone()),
        }
    }

    pub(crate) fn confirm(&self, config: &Config) -> Result<()> {
        if let Some(expected) = &self.expected {
            current_job(config, expected)?;
        }
        Ok(())
    }

    pub(crate) fn persist(&mut self, config: &Config, job: &Job) -> Result<()> {
        self.confirm(config)?;
        storage::write(&super::job_path(config, &job.task.id), job)?;
        if self.expected.is_some() {
            current_job(config, job)?;
            self.expected = Some(job.clone());
        }
        Ok(())
    }
}

/// Settle only the old launch authority, never replace a newer durable Job.
pub(crate) fn launch_failed(
    config: &Config,
    workflow: &workflow::Workflow,
    job: &mut Job,
    writes: &mut JobWrites,
    error: &anyhow::Error,
    guarded: bool,
) -> Result<()> {
    let _cleanup = process::cleanup_scope();
    if let Err(drift) = writes.confirm(config) {
        if let Some(launch) = &job.launch {
            // The old claim must not settle a different current attempt.
            // Release only the exact token retained by this launch path.
            let _ = workflow.release(&launch.lease);
        }
        return Err(drift.context(format!("guarded launch failed: {error:#}")));
    }
    let mut release_error = None;
    if let Some(launch) = job.launch.clone() {
        let observation = super::ExitObservation {
            success: false,
            code: None,
            reason: Some(format!("launch failed: {error:#}")),
        };
        let _ = storage::write(
            &workflow
                .work_dir
                .join("host-exits")
                .join(format!("{}.json", job.run_id)),
            &observation,
        );
        if super::settle(workflow, job, &observation).is_ok() {
            job.launch = None;
        }
        release_error = workflow.release(&launch.lease).err();
    }
    if guarded {
        job.last_error = Some(match release_error {
            Some(release) => format!(
                "guarded launch failed; lease reconciliation required: {error:#}; {release:#}"
            ),
            None if job.launch.is_some() => {
                format!("guarded launch requires reconciliation: {error:#}")
            }
            None => format!("guarded launch failed; explicit reconciliation required: {error:#}"),
        });
        job.retry_after = None;
    } else if job.launch.is_none() {
        super::defer_read_retry(job, format!("launch failed: {error:#}"));
    } else {
        job.last_error = Some(format!("launch requires reconciliation: {error:#}"));
        job.retry_after = None;
    }
    writes.persist(config, job)
}
