//! Stable strategy activation, including freshness checks before any successful return.
use crate::{
    files::read_strict,
    paths::{
        check_gate_paths, ensure_host_path, relative_to, resolve_destination, resolve_isolation,
    },
    protocol::StrategyPromotionRequest,
    receipts::{EvaluationKind, EvaluationReceipt},
    selection::{SelectedStrategy, StrategySelections},
};
use anyhow::{Context, Result, ensure};
use evolution::{EvaluationContext, PromptRegistry};
use freshness::RemoteClient;
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

fn descends_from(registry: &PromptRegistry, candidate: &str, baseline: &str) -> bool {
    let mut pending = vec![candidate];
    let mut seen = BTreeSet::new();
    while let Some(version) = pending.pop() {
        if !seen.insert(version) {
            continue;
        }
        if let Some(prompt) = registry.get(version) {
            for parent in &prompt.definition().parent_versions {
                if parent == baseline {
                    return true;
                }
                pending.push(parent);
            }
        }
    }
    false
}

pub(super) fn run(
    input: &Path,
    base: &Path,
    output: Option<PathBuf>,
    remote: &RemoteClient,
) -> Result<Value> {
    let mut request: StrategyPromotionRequest = read_strict(input)?;
    resolve_isolation(base, &mut request.holdout_isolation);
    let context_path = relative_to(base, &request.context_file);
    let registry_path = relative_to(base, &request.registry_file);
    let receipt_path = relative_to(base, &request.evaluation_receipt);
    let destination = output.context("promote requires an explicit --output selections path")?;
    check_gate_paths(
        input,
        &context_path,
        Some(&destination),
        &request.holdout_isolation,
    )?;
    for source in [&registry_path, &receipt_path] {
        ensure_host_path(source, &request.holdout_isolation)?;
        ensure!(
            resolve_destination(source)? != destination,
            "selection output cannot replace registry or receipt"
        );
    }
    let context: EvaluationContext = read_strict(&context_path)?;
    let receipt: EvaluationReceipt = read_strict(&receipt_path)?;
    receipt.validate()?;
    ensure!(
        receipt.kind == EvaluationKind::Promotion && receipt.decision.approved,
        "candidate has not passed the prompt promotion gate"
    );
    ensure!(
        serde_json::to_value(&context)? == serde_json::to_value(&receipt.context)?,
        "promotion receipt is stale for the expected host context"
    );
    ensure!(
        context.candidate.prompt_version == request.candidate_version,
        "receipt approves a different prompt version"
    );
    ensure!(
        context.baseline.source_commit == context.candidate.source_commit
            && context.baseline.configuration_digest == context.candidate.configuration_digest,
        "prompt activation requires unchanged source and model/tool/memory configuration"
    );
    let registry: PromptRegistry = read_strict(&registry_path)?;
    registry.validate()?;
    let candidate = registry
        .get(&request.candidate_version)
        .context("candidate prompt is not registered")?;
    let baseline = registry
        .get(&context.baseline.prompt_version)
        .context("baseline prompt is not registered")?;
    ensure!(
        candidate.definition().role == request.role && baseline.definition().role == request.role,
        "promotion role does not match registered prompt roles"
    );
    ensure!(
        candidate.definition().task_kind == baseline.definition().task_kind,
        "promotion changes task kind"
    );
    ensure!(
        descends_from(
            &registry,
            &request.candidate_version,
            &context.baseline.prompt_version
        ),
        "candidate does not descend from the evaluated baseline"
    );
    let _lock = storage::lock(&destination.with_extension("evolution.lock"))?;
    let mut selections: StrategySelections = if destination.exists() {
        read_strict(&destination)?
    } else {
        StrategySelections::default()
    };
    selections.validate(&registry)?;
    let upstream = receipt
        .upstream
        .as_ref()
        .context("missing promotion upstream baseline")?;
    // The paired run stays frozen, but its historical success cannot
    // authorize selection after the remote default branch advances.
    remote.verify_remote_binding(upstream)?;
    let skills = receipt
        .skills_manifest
        .as_ref()
        .context("missing promotion skills manifest")?;
    ensure_host_path(&skills.path, &request.holdout_isolation)?;
    skills.verify_skills(remote)?;
    if let Some(current) = selections.roles.get(&request.role) {
        if current.stable_version == request.candidate_version {
            ensure!(
                current.evidence_digest == receipt.decision.evidence_digest
                    && current.policy_version == context.candidate.policy_version,
                "selected candidate is bound to a different promotion receipt"
            );
            return Ok(
                json!({"selected":true,"already_selected":true,"role":request.role,"version":request.candidate_version,"selections_file":destination}),
            );
        }
        ensure!(
            current.stable_version == context.baseline.prompt_version,
            "stale promotion: evaluated baseline is no longer the selected strategy"
        );
        ensure!(
            current.policy_version == context.candidate.policy_version,
            "formal policy changed; automatic selection cannot adopt a new policy"
        );
    }
    let mut history = selections
        .roles
        .get(&request.role)
        .map(|current| current.previous_stable_versions.clone())
        .unwrap_or_default();
    history.retain(|version| {
        version != &request.candidate_version && version != &context.baseline.prompt_version
    });
    history.insert(0, context.baseline.prompt_version.clone());
    history.truncate(5);
    selections.roles.insert(
        request.role.clone(),
        SelectedStrategy {
            stable_version: request.candidate_version.clone(),
            task_kind: candidate.definition().task_kind.clone(),
            previous_stable_versions: history,
            evidence_digest: receipt.decision.evidence_digest,
            policy_version: context.candidate.policy_version,
        },
    );
    selections.validate(&registry)?;
    skills.verify_skills(remote)?;
    remote.verify_remote_binding(upstream)?;
    storage::write(&destination, &selections)?;
    Ok(
        json!({"selected":true,"already_selected":false,"role":request.role,"version":request.candidate_version,"selections_file":destination}),
    )
}

#[cfg(test)]
#[path = "promotion_tests.rs"]
mod tests;
