//! Preflight every frozen job before replayable registration through the runtime.
use crate::{
    artifacts::{immutable_write, private_path},
    bindings::check_job,
    identity::input_digest,
    planning::validate_plan,
    protocol::{Plan, PlannedTask},
};
use anyhow::{Context, Result, ensure};
use config::Config;
use inputs::ExecutionInputs;
use serde_json::{Value, json};
use std::path::Path;
use task::Job;

fn enqueue_summary(plan: &Plan) -> Value {
    json!({"schema_version":1,"plan_digest":plan.digest,"input_digest":plan.input_digest,
        "task_ids":plan.tasks.iter().map(|entry| &entry.task.id).collect::<Vec<_>>(),
        "registered_tasks":plan.tasks.len(),"worker_launch_requested":false})
}

fn check_registered_job(c: &Config, plan: &Plan, entry: &PlannedTask, job: &Job) -> Result<()> {
    check_job(c, plan, entry, job)?;
    ensure!(
        job.baseline_commit.is_none() && job.run_id == format!("{}-attempt-1", entry.task.id),
        "existing batch job has a different frozen run or candidate baseline"
    );
    // Re-render the job's original checked_at values, never the new refresh.
    let prompt = runtime::rendered_job_prompt(c, job)?;
    ensure!(
        storage::digest(prompt.as_bytes()) == job.prompt_digest,
        "existing job prompt binding differs from its frozen inputs"
    );
    Ok(())
}

pub(super) fn enqueue_plan(
    c: &Config,
    plan: &Plan,
    refresh: impl FnOnce() -> Result<ExecutionInputs>,
) -> Result<Value> {
    validate_plan(c, plan)?;
    for prompt in &plan.prompts {
        let registered = evolution_cli::resolve_registered_prompt(
            &c.state_dir.join("evolution/prompts.json"),
            prompt.version(),
            &prompt.definition().role,
        )?;
        ensure!(
            registered.version() == prompt.version(),
            "registered prompt differs from plan"
        );
    }
    // Reject every existing mismatch before creating any new job or worktree.
    for entry in &plan.tasks {
        let path = c
            .state_dir
            .join("jobs")
            .join(format!("{}.json", entry.task.id));
        if path.exists() {
            let job: Job = storage::read(&path)?;
            check_registered_job(c, plan, entry, &job)?;
        }
    }
    let inputs = refresh()?;
    ensure!(
        input_digest(&inputs)? == plan.input_digest,
        "manifest source, skill or knowledge baseline is stale; create a new experiment"
    );
    for entry in &plan.tasks {
        let job = runtime::enqueue_with_inputs(c, entry.task.clone(), Some(inputs.clone()))?;
        // Also detects a conflicting ordinary enqueue racing with preflight.
        check_registered_job(c, plan, entry, &job)?;
    }
    Ok(enqueue_summary(plan))
}

pub(super) fn execute(c: &Config, input: &Path, output: &Path) -> Result<Value> {
    private_path(c, input, false)?;
    private_path(c, output, true)?;
    let plan: Plan = storage::read(input)?;
    let _lock = storage::lock(&c.state_dir.join("collaboration-enqueue.lock"))?;
    let expected = enqueue_summary(&plan);
    if output.exists() {
        ensure!(
            storage::read::<Value>(output)? == expected,
            "batch output already binds a different plan"
        );
    }
    let result = enqueue_plan(c, &plan, || {
        inputs::collect(c)?
            .context("batch registration requires current upstream and skill bindings")
    })?;
    immutable_write(c, output, &result)?;
    Ok(result)
}

#[cfg(test)]
#[path = "registration_tests.rs"]
mod tests;
