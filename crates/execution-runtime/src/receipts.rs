//! Workflow-committed evidence is the authority for dependencies and completed results.
use crate::{job_path, workflow};
use anyhow::{Context, Result, ensure};
use config::Config;
use contracts::WorkflowState;
use native_effects::fs;
use serde_json::{Value, json};
use task::Job;
use workflow::Workflow;

pub(super) fn validate_report(value: &Value) -> Result<()> {
    let object = value
        .as_object()
        .context("agent result must be an object")?;
    ensure!(
        object.keys().all(|key| [
            "summary",
            "findings",
            "sources",
            "limitations",
            "next_tasks"
        ]
        .contains(&key.as_str()))
            && value["summary"]
                .as_str()
                .is_some_and(|s| !s.trim().is_empty()),
        "malformed agent summary or unexpected fields"
    );
    for key in ["findings", "sources", "limitations"] {
        ensure!(
            value[key]
                .as_array()
                .is_some_and(|a| a.iter().all(Value::is_string)),
            "malformed agent {key}"
        );
    }
    Ok(())
}

pub(super) fn committed_receipt(c: &Config, w: &Workflow, job: &Job) -> Result<Value> {
    let state = w.status(&job.run_id)?;
    ensure!(
        WorkflowState::from_value(&state["status"]) == Some(WorkflowState::Succeeded),
        "dependency has not succeeded"
    );
    let committed = state["frames"]["1"]["nodes"]["task"]["outputs"]["result"]
        .as_str()
        .context("dependency lacks committed worker output")?;
    let receipt: Value = serde_json::from_str(committed)?;
    let local: Value = storage::read(
        &c.state_dir
            .join("runs")
            .join(&job.run_id)
            .join("receipt.json"),
    )?;
    ensure!(
        local == receipt,
        "local dependency receipt differs from authoritative output"
    );
    Ok(receipt)
}

pub(super) fn completed_dependencies(c: &Config, w: &Workflow, job: &Job) -> Result<Value> {
    let mut results = vec![];
    let mut bytes = 0usize;
    for id in &job.task.dependencies {
        let dependency: Job = storage::read(&job_path(c, id))?;
        let state = w.status(&dependency.run_id)?;
        ensure!(
            WorkflowState::from_value(&state["status"]) == Some(WorkflowState::Succeeded),
            "dependency {id} has not succeeded"
        );
        let receipt_path = c
            .state_dir
            .join("runs")
            .join(&dependency.run_id)
            .join("receipt.json");
        ensure!(
            fs::metadata(&receipt_path)?.len() <= 256 * 1024,
            "dependency report {id} exceeds 256 KiB"
        );
        let receipt: Value = storage::read(&receipt_path)?;
        validate_report(&receipt["agent_report"])?;
        let committed = state["frames"]["1"]["nodes"]["task"]["outputs"]["result"]
            .as_str()
            .context("dependency lacks committed worker output")?;
        ensure!(
            serde_json::from_str::<Value>(committed)? == receipt,
            "dependency receipt differs from workflow-committed output"
        );
        ensure!(
            receipt["source_commit"] == dependency.source_commit
                && receipt["superpod_commit"] == dependency.superpod_commit
                && receipt["prompt_digest"] == dependency.prompt_digest,
            "dependency receipt bindings changed"
        );
        bytes += serde_json::to_vec(&receipt)?.len();
        ensure!(
            bytes <= 512 * 1024,
            "combined dependency reports exceed 512 KiB"
        );
        results.push(
            json!({"id":id,"run_id":dependency.run_id,"model":dependency.model,"receipt":receipt}),
        );
    }
    Ok(Value::Array(results))
}

/// The workflow output is authoritative even if a crash interrupted publication
/// of the local receipt or the completion queue. Agent-writable logs are views.
pub(super) fn authoritative_success_receipt(w: &Workflow, job: &Job) -> Result<Value> {
    let state = w.status(&job.run_id)?;
    ensure!(
        WorkflowState::from_value(&state["status"]) == Some(WorkflowState::Succeeded),
        "completion parent has not succeeded"
    );
    let encoded = state["frames"]["1"]["nodes"]["task"]["outputs"]["result"]
        .as_str()
        .context("successful workflow lacks its committed receipt")?;
    ensure!(
        encoded.len() <= 2 * 1024 * 1024,
        "committed receipt exceeds 2 MiB"
    );
    let receipt: Value = serde_json::from_str(encoded)?;
    validate_report(&receipt["agent_report"])?;
    ensure!(
        receipt["receipt_protocol"] == 2 && receipt["config_digest"] == job.config_digest,
        "committed completion receipt does not bind the frozen policy"
    );
    ensure!(
        receipt["source_commit"] == job.source_commit
            && receipt["superpod_commit"] == job.superpod_commit
            && receipt["prompt_digest"] == job.prompt_digest
            && receipt["model"] == job.model
            && receipt["research_inputs"] == serde_json::to_value(&job.research_inputs)?
            && receipt["backend"] == serde_json::to_value(&job.backend)?,
        "committed completion receipt does not bind the frozen experiment"
    );
    Ok(receipt)
}

/// Scenario adapters may project only the same validated, workflow-committed result
/// used by completion processing. Raw worker files are never result authority.
pub fn completed_result(c: &Config, job: &Job) -> Result<Value> {
    authoritative_success_receipt(&workflow(c), job)
}

#[cfg(test)]
#[path = "receipts_tests.rs"]
mod tests;
