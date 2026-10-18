//! Task admission, immutable enqueue identity and bounded read retries.
use crate::host_budget::now;
use crate::job_store::{ensure_started, job_path, workflow};
use crate::people;
use crate::registration;
use anyhow::{Context, Result, ensure};
use config::{Config, safe_id};
use contracts::WorkflowState;
use serde_json::{Value, json};
use task::{Job, Task};

pub fn enqueue(c: &Config, task: Task) -> Result<Job> {
    safe_id(&task.id)?;
    let inputs = if job_path(c, &task.id).exists() {
        None
    } else {
        inputs::collect(c)?
    };
    enqueue_with_inputs(c, task, inputs)
}

/// Enqueue a host-owned task with an already collected, immutable input snapshot.
/// Batch callers must bind this snapshot to their validated plan; ordinary callers
/// should use `enqueue`. Execution still verifies freshness before admission.
pub fn enqueue_with_inputs(
    c: &Config,
    mut task: Task,
    inputs: Option<inputs::ExecutionInputs>,
) -> Result<Job> {
    safe_id(&task.id)?;
    if task.prompt_version.is_none() {
        task.prompt_version = if job_path(c, &task.id).exists() {
            storage::read::<Job>(&job_path(c, &task.id))?
                .task
                .prompt_version
        } else {
            evolution_cli::resolve_selection(&c.state_dir, &task.role)?
        };
    }
    if let Some(team) = &task.communication {
        team.validate()?;
    }
    ensure!(!task.prompt.trim().is_empty(), "empty task prompt");
    ensure!(task.depth <= 3, "follow-up task depth exceeds three");
    ensure!(
        task.max_attempts
            .is_none_or(|limit| (1..=3).contains(&limit)),
        "task max_attempts must be 1..3"
    );
    ensure!(c.models.contains_key(&task.role), "unknown model role");
    ensure!(
        !task.dependencies.contains(&task.id),
        "task depends on itself"
    );
    for d in &task.dependencies {
        safe_id(d)?;
        ensure!(job_path(c, d).exists(), "unknown dependency {d}");
    }
    for t in &task.required_tools {
        ensure!(c.tools.contains_key(t), "unknown tool {t}");
    }
    let _lock = storage::lock(&c.state_dir.join("enqueue.lock"))?;
    let destination = job_path(c, &task.id);
    if destination.exists() {
        let prior: Job = storage::read(&destination)?;
        ensure!(
            serde_json::to_value(&prior.task)? == serde_json::to_value(&task)?,
            "task ID already binds different inputs"
        );
        agent_backend::verify_job(c, &prior)?;
        people::bind(c, &prior)?;
        ensure_started(c, &prior)?;
        return Ok(prior);
    }
    let prepared = registration::prepare(c, task, inputs.as_ref())?;
    registration::register(c, &prepared, inputs, |_, _| Ok(()))
}

fn attempt_limit(task: &Task) -> u32 {
    // Admission validates explicit limits; clamp retained malformed data fail closed.
    u32::from(task.max_attempts.unwrap_or(3).min(3))
}

pub(super) fn defer_read_retry(job: &mut Job, reason: String) {
    job.last_error = Some(reason);
    job.retry_after = if !job.task.write && job.attempt < attempt_limit(&job.task) {
        Some(now() + 5 * (1_i64 << (job.attempt - 1)))
    } else {
        None
    };
}

pub(super) fn prepare_retry(c: &Config, job: &mut Job) -> Result<()> {
    ensure!(
        !job.task.write && job.attempt < attempt_limit(&job.task) && job.launch.is_none(),
        "retry is not admissible"
    );
    job.attempt += 1;
    job.run_id = format!("{}-attempt-{}", job.task.id, job.attempt);
    job.last_error = None;
    job.retry_after = None;
    // Record the new run identity before starting it; startup can repair a missing start.
    storage::write(&job_path(c, &job.task.id), job)?;
    ensure_started(c, job)
}

pub fn retry(c: &Config, id: &str) -> Result<Value> {
    let _lock = storage::lock(&c.state_dir.join("controller.lock"))?;
    safe_id(id)?;
    let mut job: Job = storage::read(&job_path(c, id))?;
    ensure!(
        !job.task.write && job.attempt < attempt_limit(&job.task),
        "write retry requires reconciliation; read task attempt limit reached"
    );
    let w = workflow(c);
    let state = w.status(&job.run_id)?;
    ensure!(
        WorkflowState::from_value(&state["status"]) != Some(WorkflowState::Succeeded)
            && job.launch.is_none(),
        "cannot retry active/completed task"
    );
    if WorkflowState::from_value(&state["status"]) == Some(WorkflowState::Running)
        && state.get("pause").is_none_or(Value::is_null)
    {
        w.pause(
            &job.run_id,
            &format!("retry-{}", workflow::now_ms()?),
            state["revision"]
                .as_u64()
                .context("missing workflow revision")?,
            workflow::now_ms()?,
            "read-only retry supersedes old run",
        )?;
    }
    prepare_retry(c, &mut job)?;
    Ok(json!({"run_id":job.run_id}))
}

#[cfg(test)]
#[path = "jobs_tests.rs"]
mod tests;
