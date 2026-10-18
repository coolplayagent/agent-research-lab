//! Prove the installed candidate or baseline without replaying deployment effects.
use crate::{
    admission::{confirmed_claim, release_drain, storage_client},
    host::command,
    journal::{bound_receipt, phase},
    model::{Prepared, SwitchProof},
    service::{
        config_binding, config_loaded_after_write, running_artifact, service_health, systemctl,
    },
};
use anyhow::{Context, Result, ensure};
use config::Config;
use contracts::{ActivationPhase, ActivationState, IterationState};
use serde_json::{Value, json};
use std::{fs, path::Path, time::Duration};

fn verify_installation(prepared: &Prepared, proof: &SwitchProof, active: bool) -> Result<()> {
    let expected_config = if active {
        &proof.next_config_sha256
    } else {
        &proof.previous_config_sha256
    };
    ensure!(
        delivery::file_sha256(&prepared.policy.service_config)? == *expected_config,
        "installed configuration does not match activation proof"
    );
    Config::load(&prepared.policy.service_config)?;
    let unit_text = String::from_utf8(systemctl(&[
        "cat".into(),
        prepared.policy.main_unit.clone(),
    ])?)?;
    config_binding(&unit_text, &prepared.policy.service_config)?;
    if proof.units.is_empty() {
        ensure!(
            running_artifact(&prepared.policy.main_unit)? == proof.main_artifact_sha256,
            "running main service differs from the activation proof"
        );
        config_loaded_after_write(&prepared.policy)?;
        if active {
            ensure!(
                delivery::file_sha256(&proof.artifact)? == proof.artifact_sha256,
                "installed CLI artifact differs from activation proof"
            );
            command(
                &[
                    vec![proof.artifact.display().to_string()],
                    prepared.repository.probe.clone(),
                ]
                .concat(),
                proof
                    .artifact
                    .parent()
                    .context("artifact directory missing")?,
                30,
            )?;
        }
    } else {
        ensure!(
            proof.units.len() == 4,
            "platform proof must cover all services"
        );
        for item in &proof.units {
            let expected_unit = if active {
                &item.next_unit_sha256
            } else {
                &item.previous_unit_sha256
            };
            let expected_binary = if active {
                &item.next_artifact_sha256
            } else {
                &item.previous_artifact_sha256
            };
            ensure!(
                delivery::file_sha256(&item.unit_file)? == *expected_unit
                    && running_artifact(&item.unit)? == *expected_binary,
                "platform installation differs from activation proof"
            );
            systemctl(&["is-active".into(), item.unit.clone()])?;
        }
        let client = storage_client(prepared)?;
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(client.probe())?;
    }
    service_health(&prepared.policy)
}

/// Inspect exact installation proofs without replaying any deployment side effects.
pub fn reconcile(operation: &Path) -> Result<Value> {
    let operation = fs::canonicalize(operation)?;
    let prepared: Prepared = storage::read(&operation.join("prepared.json"))?;
    let _lock = storage::lock(
        &prepared
            .state_dir
            .join("private/software-activation-worker.lock"),
    )?;
    let _deadline = process::deadline_scope(Duration::from_secs(300));
    let Some(claim) = confirmed_claim(&operation, &prepared)? else {
        release_drain(&operation, &prepared)?;
        return Ok(
            json!({"schema_version":1,"goal_id":prepared.request.goal_id,
            "candidate_commit":prepared.request.candidate_commit,"merged_commit":prepared.request.merged_commit,
            "active":false,"phase":ActivationPhase::Blocked,"requires_reconciliation":false,
            "details":{"error":"Activation admission was rejected before any switch began."}}),
        );
    };
    let client = storage_client(&prepared)?;
    let run = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(client.evolution_run(&prepared.request.iteration_id))?;
    let round = run.rounds.last().context("iteration round missing")?;
    ensure!(
        run.state == IterationState::Running
            && round.goal_id == prepared.request.goal_id
            && round
                .activation
                .as_ref()
                .is_some_and(|activation| activation.state == ActivationState::Pending
                    && activation.reference == claim),
        "reconciliation no longer owns the pending activation"
    );
    let proof: SwitchProof = storage::read(&operation.join("switch-proof.json"))?;
    let active = match verify_installation(&prepared, &proof, true) {
        Ok(()) => true,
        Err(active_error) => {
            verify_installation(&prepared, &proof, false).with_context(||
                format!("neither candidate nor baseline installation can be verified; candidate: {active_error}"))?;
            false
        }
    };
    let receipt = bound_receipt(
        &operation,
        &prepared,
        active,
        &proof.artifact,
        &proof.artifact_sha256,
    )?;
    storage::write(&operation.join("receipt.json"), &receipt)?;
    phase(
        &operation,
        if active {
            ActivationPhase::Active
        } else {
            ActivationPhase::RolledBack
        },
        json!({"reconciled":true}),
    )?;
    release_drain(&operation, &prepared)?;
    Ok(receipt)
}

#[cfg(test)]
#[path = "reconcile_tests.rs"]
mod tests;
