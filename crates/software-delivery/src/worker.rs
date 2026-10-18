//! Serialize bounded workers and coordinate verified activation stages.
use crate::{
    admission::{confirmed_claim, release_drain},
    build::build,
    cli::activate_cli,
    journal::phase,
    model::Prepared,
    platform::activate_platform,
    service::{config_binding, systemctl},
};
use anyhow::{Result, ensure};
use config::Config;
use contracts::ActivationPhase;
use serde_json::{Value, json};
use std::{fs, path::Path, time::Duration};

pub fn worker(operation: &Path) -> Result<Value> {
    let operation = fs::canonicalize(operation)?;
    let prepared: Prepared = storage::read(&operation.join("prepared.json"))?;
    let _lock = storage::lock(
        &prepared
            .state_dir
            .join("private/software-activation-worker.lock"),
    )?;
    let _deadline = process::deadline_scope(Duration::from_secs(7000));
    let result = activate(&operation, &prepared);
    match result {
        Ok(receipt) => {
            storage::write(&operation.join("receipt.json"), &receipt)?;
            phase(&operation, ActivationPhase::Active, json!({}))?;
            release_drain(&operation, &prepared)?;
            Ok(receipt)
        }
        Err(error) => {
            let current: Value = storage::read(&operation.join("status.json"))?;
            let rolled_back =
                ActivationPhase::from_value(&current["phase"]) == Some(ActivationPhase::RolledBack);
            if rolled_back
                || confirmed_claim(&operation, &prepared).is_ok_and(|claim| claim.is_none())
            {
                release_drain(&operation, &prepared)?;
            }
            if !rolled_back {
                phase(
                    &operation,
                    ActivationPhase::Blocked,
                    json!({"error":error.to_string()}),
                )?;
            }
            Err(error)
        }
    }
}

fn activate(operation: &Path, prepared: &Prepared) -> Result<Value> {
    ensure!(
        delivery::file_sha256(&prepared.policy.service_config)? == prepared.config_sha256,
        "service config changed before activation"
    );
    ensure!(
        delivery::file_sha256(&prepared.state_dir.join("private/software-activation.json"))?
            == prepared.policy_sha256,
        "activation policy changed before execution"
    );
    let config = Config::load(&prepared.policy.service_config)?;
    let unit_text = String::from_utf8(systemctl(&[
        "cat".into(),
        prepared.policy.main_unit.clone(),
    ])?)?;
    config_binding(&unit_text, &prepared.policy.service_config)?;
    let (source, _) = build(operation, prepared, &config)?;
    if !prepared.repository.platform_units.is_empty() {
        return activate_platform(operation, prepared, &config, &source);
    }
    activate_cli(operation, prepared, &config, &source)
}

#[cfg(test)]
#[path = "worker_tests.rs"]
mod tests;
