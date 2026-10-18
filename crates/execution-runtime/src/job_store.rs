//! Durable research jobs and replayable workflow starts.
use anyhow::Result;
use config::Config;
use native_effects::fs;
use serde_json::{Value, json};
use std::path::PathBuf;
use task::Job;
use workflow::Workflow;

pub(super) fn workflow(c: &Config) -> Workflow {
    Workflow::new(
        &c.workflow,
        c.state_dir.join("workflow.sqlite"),
        c.state_dir.join("workflow"),
    )
}

pub(super) fn job_path(c: &Config, id: &str) -> PathBuf {
    c.state_dir.join("jobs").join(format!("{id}.json"))
}

pub(super) fn jobs(c: &Config) -> Result<Vec<Job>> {
    let directory = c.state_dir.join("jobs");
    if !directory.exists() {
        return Ok(vec![]);
    }
    let mut paths = fs::read_dir(directory)?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    paths.sort();
    paths
        .into_iter()
        .filter(|p| p.extension().is_some_and(|s| s == "json"))
        .map(|p| storage::read(&p))
        .collect()
}

pub(super) fn frozen_input(job: &Job) -> Value {
    let mut value = json!({"task":job.task,"model":job.model,"source_commit":job.source_commit,
        "superpod_commit":job.superpod_commit,"prompt_digest":job.prompt_digest,
        "config_digest":job.config_digest,"worktree":job.worktree});
    if let Some(persona) = &job.persona {
        value["persona"] = json!(persona);
    }
    if let Some(backend) = &job.backend {
        value["backend"] = json!(backend);
    }
    if let Some(base) = &job.baseline_commit {
        value["baseline_commit"] = json!(base);
    }
    if let Some(inputs) = &job.research_inputs {
        value["research_inputs"] = json!(inputs);
    }
    value
}

pub(super) fn ensure_started(c: &Config, job: &Job) -> Result<()> {
    let w = workflow(c);
    w.initialize()?;
    ensure_started_in_store(c, &w, job)
}

pub(super) fn ensure_started_in_store(c: &Config, w: &Workflow, job: &Job) -> Result<()> {
    if w.replay_task_start(&job.run_id, job.task.write, frozen_input(job))?
        .is_some()
    {
        return Ok(());
    }
    w.start_task(
        &job.run_id,
        job.task.write,
        frozen_input(job),
        c.task_timeout_seconds * 1000,
    )?;
    Ok(())
}

#[cfg(test)]
#[path = "job_store_tests.rs"]
mod tests;
