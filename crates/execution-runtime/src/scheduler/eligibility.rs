//! Shared cohort ordering and fail-closed workflow admission checks.
use crate::job_store::job_path;
use anyhow::Result;
use config::Config;
use contracts::WorkflowState;
use task::Job;
use workflow::Workflow;

/// Sandbox verification must enter without an earlier peer. An ordinary task
/// may follow that first worker, but every member shares its exploration track.
pub(super) fn may_join(first: Option<&Job>, next: &Job) -> bool {
    first.is_none_or(|first| {
        next.task.exploratory == first.task.exploratory
            && !next
                .task
                .required_tools
                .iter()
                .any(|tool| tool == "repo-sandbox")
    })
}

/// Recheck durable admission before preparation and again before a claim.
/// Dependency errors or future protocol states cannot authorize execution.
pub(super) fn admission_ready(c: &Config, w: &Workflow, job: &Job) -> Result<bool> {
    let state = w.status(&job.run_id)?;
    if WorkflowState::from_value(&state["status"]) != Some(WorkflowState::Running)
        || state.get("pause").is_some_and(|value| !value.is_null())
    {
        return Ok(false);
    }
    Ok(!job.task.dependencies.iter().any(|id| {
        storage::read::<Job>(&job_path(c, id))
            .and_then(|dependency| w.status(&dependency.run_id))
            .map(|state| {
                WorkflowState::from_value(&state["status"]) != Some(WorkflowState::Succeeded)
            })
            .unwrap_or(true)
    }))
}
