//! Confirm durable goal ownership and hold native admission while switching.
use crate::{
    journal::{activation_claim, phase},
    model::Prepared,
};
use anyhow::{Context, Result, ensure};
use config::Config;
use contracts::{ActivationPhase, ActivationState, CandidateState, IterationState, RoundState};
use serde_json::{Value, json};
use std::{
    fs,
    path::Path,
    time::{Duration, Instant},
};

pub(super) fn storage_client(prepared: &Prepared) -> Result<collaboration_client::Client> {
    collaboration_client::Client::remote(
        prepared
            .policy
            .storage_socket
            .clone()
            .context("activation requires the collaboration storage endpoint")?,
    )
}

pub(super) fn confirmed_claim(operation: &Path, prepared: &Prepared) -> Result<Option<String>> {
    let Some(claim) = activation_claim(operation)? else {
        return Ok(None);
    };
    let client = storage_client(prepared)?;
    let run = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(client.evolution_run(&prepared.request.iteration_id))?;
    let round = run
        .rounds
        .iter()
        .find(|round| round.goal_id == prepared.request.goal_id)
        .context("activation goal is missing from its durable iteration")?;
    confirmed_round_claim(
        round,
        &claim,
        operation.join("switch-started.json").exists(),
    )
}

fn confirmed_round_claim(
    round: &collaboration_model::EvolutionRound,
    claim: &str,
    switched: bool,
) -> Result<Option<String>> {
    if round.activation.as_ref().is_some_and(|activation| {
        activation.state == ActivationState::Pending && activation.reference == claim
    }) {
        return Ok(Some(claim.into()));
    }
    ensure!(
        round.activation.is_none() && !switched,
        "activation admission does not match its durable round; reconcile actual service state"
    );
    Ok(None)
}

pub(super) fn begin_activation(operation: &Path, prepared: &Prepared) -> Result<String> {
    let client = storage_client(prepared)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let run = client.evolution_run(&prepared.request.iteration_id).await?;
        let round = run
            .rounds
            .last()
            .context("activation iteration has no round")?;
        let expected_version = format!(
            "candidate-{}",
            &storage::digest(
                format!(
                    "{}:{}",
                    run.input.subject_id, prepared.request.candidate_commit
                )
                .as_bytes()
            )[..40]
        );
        ensure!(
            run.state == IterationState::Running
                && round.state == RoundState::Accepted
                && round.goal_id == prepared.request.goal_id
                && round.finished_ms.is_none()
                && round
                    .candidate
                    .as_ref()
                    .is_some_and(|candidate| candidate.state == CandidateState::Accepted
                        && candidate.version_id == expected_version),
            "activation no longer owns the running accepted candidate"
        );
        let claim_id = format!(
            "activation-{}",
            &storage::digest(serde_json::to_vec(&prepared.request)?.as_slice())[..40]
        );
        let change = collaboration_model::EvolutionChange {
            request_id: claim_id.clone(),
            run_id: run.input.id,
            revision: run.revision,
            action: collaboration_model::EvolutionAction::BeginActivation {
                claim_id: claim_id.clone(),
                max_seconds: 3600,
            },
        };
        // Persist the exact command before RPC; an interrupted response cannot erase admission.
        storage::write(&operation.join("admission.json"), &change)?;
        client.evolution_change(change).await?;
        Ok(claim_id)
    })
}

fn native_idle(config: &Config) -> Result<bool> {
    let directory = config.state_dir.join("jobs");
    if !directory.exists() {
        return Ok(true);
    }
    for (index, entry) in fs::read_dir(directory)?.enumerate() {
        ensure!(index < 16384, "activation job scan exceeds bound");
        let entry = entry?;
        if entry
            .path()
            .extension()
            .is_some_and(|value| value == "json")
        {
            let job: task::Job = storage::read(&entry.path())?;
            if job.launch.is_some() {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

pub(super) fn drain(
    operation: &Path,
    prepared: &Prepared,
    config: &Config,
) -> Result<storage::LockGuard> {
    phase(operation, ActivationPhase::Draining, json!({}))?;
    storage::write(
        &prepared.state_dir.join("private/activation-drain"),
        &json!({"operation":operation,"goal_id":prepared.request.goal_id}),
    )?;
    let started = Instant::now();
    loop {
        if let Ok(guard) = storage::lock(&config.state_dir.join("private/software-admission.lock"))
            && native_idle(config)?
        {
            return Ok(guard);
        }
        ensure!(
            started.elapsed() < Duration::from_secs(3600),
            "activation drain deadline exceeded"
        );
        std::thread::sleep(Duration::from_secs(2));
    }
}

pub(super) fn release_drain(operation: &Path, prepared: &Prepared) -> Result<()> {
    let drain = prepared.state_dir.join("private/activation-drain");
    if drain.exists()
        && storage::read::<Value>(&drain)?["operation"] == operation.display().to_string()
    {
        fs::remove_file(drain)?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "admission_tests.rs"]
mod tests;
