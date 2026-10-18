//! Bind one planned call to its registered job and workflow-committed receipt.
use crate::{
    identity::input_digest,
    protocol::{Plan, PlannedTask},
};
use anyhow::{Context, Result, ensure};
use config::Config;
use serde_json::Value;
use task::Job;

pub(super) fn check_job(c: &Config, plan: &Plan, entry: &PlannedTask, job: &Job) -> Result<()> {
    ensure!(
        job.attempt == 1,
        "retry exceeds the pilot fixed-call contract; start a new experiment"
    );
    ensure!(
        serde_json::to_value(&job.task)? == serde_json::to_value(&entry.task)?,
        "task differs from immutable plan"
    );
    ensure!(
        job.model == entry.model && job.config_digest == plan.config_digest,
        "model/configuration binding changed"
    );
    ensure!(
        job.source_commit == plan.inputs.repositories["superpod"].upstream.commit
            && job.superpod_commit == job.source_commit,
        "source/SuperPOD binding changed"
    );
    ensure!(
        input_digest(
            job.research_inputs
                .as_ref()
                .context("job lacks current research bindings")?
        )? == plan.input_digest,
        "job belongs to another source/skill cohort"
    );
    ensure!(
        job.worktree == c.state_dir.join("worktrees").join(&entry.task.id),
        "job worktree changed"
    );
    Ok(())
}

pub(super) fn check_receipt(
    c: &Config,
    plan: &Plan,
    entry: &PlannedTask,
    job: &Job,
    status: &Value,
    local: &Value,
) -> Result<()> {
    check_job(c, plan, entry, job)?;
    let authoritative = status["frames"]["1"]["nodes"]["task"]["outputs"]["result"]
        .as_str()
        .context("workflow lacks committed result")?;
    ensure!(
        serde_json::from_str::<Value>(authoritative)? == *local,
        "local result differs from workflow-committed result"
    );
    ensure!(
        local["source_commit"] == job.source_commit
            && local["superpod_commit"] == job.superpod_commit
            && local["prompt_digest"] == job.prompt_digest
            && local["model"] == job.model,
        "receipt binding differs from registered job"
    );
    let version = entry
        .task
        .prompt_version
        .as_ref()
        .context("plan lacks a frozen prompt")?;
    let registered = evolution_cli::resolve_registered_prompt(
        &c.state_dir.join("evolution/prompts.json"),
        version,
        &entry.task.role,
    )?;
    ensure!(
        plan.prompts
            .iter()
            .any(|p| p.version() == registered.version()),
        "prompt is not part of this plan"
    );
    Ok(())
}

#[cfg(test)]
#[path = "bindings_tests.rs"]
mod tests;
