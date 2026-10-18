//! Durable local admission intent, switch marker, phases and bound receipts.
use crate::model::Prepared;
use anyhow::{Context, Result};
use contracts::ActivationPhase;
use serde_json::{Value, json};
use std::path::Path;

pub(super) fn activation_claim(operation: &Path) -> Result<Option<String>> {
    let path = operation.join("admission.json");
    if !path.exists() {
        return Ok(None);
    }
    let change: collaboration_model::EvolutionChange = storage::read(&path)?;
    match change.action {
        collaboration_model::EvolutionAction::BeginActivation { claim_id, .. } => {
            Ok(Some(claim_id))
        }
        _ => anyhow::bail!("invalid activation admission journal"),
    }
}

pub(super) fn mark_switch(operation: &Path) -> Result<()> {
    storage::write(
        &operation.join("switch-started.json"),
        &json!({"activation_claim_id":activation_claim(operation)?.context("activation admission missing")?,
            "started_ms":collaboration_model::now_ms()}),
    )
}

pub(super) fn bound_receipt(
    operation: &Path,
    prepared: &Prepared,
    active: bool,
    reference: &Path,
    artifact_sha256: &str,
) -> Result<Value> {
    let claim = activation_claim(operation)?.context("activation admission receipt missing")?;
    Ok(
        json!({"schema_version":1,"goal_id":prepared.request.goal_id,
        "candidate_commit":prepared.request.candidate_commit,"merged_commit":prepared.request.merged_commit,
        "active":active,"phase":if active {ActivationPhase::Active} else {ActivationPhase::RolledBack},
        "activation_claim_id":claim,"artifact_sha256":artifact_sha256,"reference":reference,
        "policy_sha256":prepared.policy_sha256,"qualitygate_sha256":delivery::file_sha256(&operation.join("merged-qualitygate.json"))?}),
    )
}

pub(super) fn phase(operation: &Path, phase: ActivationPhase, extra: Value) -> Result<()> {
    storage::write(
        &operation.join("status.json"),
        &json!({"phase":phase,"observed_ms":collaboration_model::now_ms(),"details":extra}),
    )
}

#[cfg(test)]
#[path = "journal_tests.rs"]
mod tests;
