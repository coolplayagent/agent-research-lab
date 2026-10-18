//! Receipt-backed budget batches; validate the entire batch before writing.
use crate::{
    files::read_strict,
    paths::{ensure_host_path, relative_to, resolve_destination, resolve_isolation},
    protocol::ExplorationUpdate,
    receipts::{EvaluationKind, EvaluationReceipt},
};
use anyhow::{Context, Result, ensure};
use evolution::{ExplorationBudget, ExplorationOutcome};
use freshness::RemoteClient;
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

pub(super) fn run(
    input: &Path,
    base: &Path,
    output: Option<PathBuf>,
    remote: &RemoteClient,
) -> Result<Value> {
    let mut request: ExplorationUpdate = read_strict(input)?;
    ensure!(
        !request.experiments.is_empty(),
        "exploration update needs at least one experiment receipt"
    );
    resolve_isolation(base, &mut request.holdout_isolation);
    request.holdout_isolation.validate()?;
    ensure_host_path(input, &request.holdout_isolation)?;
    let destination = output.context("exploration requires an explicit --output budget path")?;
    ensure_host_path(&destination, &request.holdout_isolation)?;
    ensure!(
        destination.as_path() != input,
        "budget output cannot replace its request file"
    );
    let _lock = storage::lock(&destination.with_extension("evolution.lock"))?;
    let mut budget: ExplorationBudget = if destination.exists() {
        read_strict(&destination)?
    } else {
        ExplorationBudget::default()
    };
    budget.validate()?;
    let mut adjustments = Vec::new();
    let mut labels = BTreeSet::new();
    let mut recorded = Vec::new();
    for experiment in request.experiments {
        ensure!(
            !experiment.experiment_id.trim().is_empty(),
            "experiment label must not be empty"
        );
        ensure!(
            labels.insert(experiment.experiment_id.clone()),
            "duplicate experiment label in update"
        );
        let receipt_path = relative_to(base, &experiment.evaluation_receipt);
        ensure_host_path(&receipt_path, &request.holdout_isolation)?;
        ensure!(
            resolve_destination(&receipt_path)? != destination,
            "receipt cannot be the budget file"
        );
        let receipt: EvaluationReceipt = read_strict(&receipt_path)?;
        receipt.validate()?;
        remote.verify_remote_binding(
            receipt
                .upstream
                .as_ref()
                .context("missing evaluation upstream")?,
        )?;
        let skills = receipt
            .skills_manifest
            .as_ref()
            .context("missing evaluation skills manifest")?;
        ensure_host_path(&skills.path, &request.holdout_isolation)?;
        skills.verify_skills(remote)?;
        let evidence_id = receipt.decision.evidence_digest.clone();
        if let Some(share) = budget.record(ExplorationOutcome {
            experiment_id: evidence_id.clone(),
            promoted_on_holdout: receipt.kind == EvaluationKind::Promotion
                && receipt.decision.approved,
            critical_regression: receipt.critical_regression(),
        })? {
            adjustments.push(share);
        }
        recorded
            .push(json!({"experiment_id":experiment.experiment_id,"evidence_digest":evidence_id}));
    }
    // Write only after every update validates; a bad batch cannot
    // partially change the durable exploration budget.
    storage::write(&destination, &budget)?;
    Ok(
        json!({"budget_file":destination,"share_percent":budget.share_percent(),"pending_count":budget.pending_count(),"completed_window_shares":adjustments,"recorded":recorded}),
    )
}

#[cfg(test)]
#[path = "exploration_tests.rs"]
mod tests;
