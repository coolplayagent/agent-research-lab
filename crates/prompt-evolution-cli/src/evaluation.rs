//! Host evaluation commands; only recomputable receipts reach durable output.
use crate::{
    files::{read_strict, write_optional},
    paths::{check_gate_paths, prepare_skills_artifact, relative_to, resolve_isolation},
    protocol::{NoveltyRequest, PromotionRequest},
    receipts::{
        EvaluationKind, EvaluationReceipt, bind_upstream_digest, canonicalize_ablations,
        canonicalize_observations, validate_evaluation_baseline,
    },
};
use anyhow::Result;
use delivery::VerificationArtifact;
use evolution::{
    EvaluationContext, HoldoutIsolation, evaluate_novel_capability, evaluate_promotion,
};
use freshness::RemoteSnapshot;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

// Preserve host authorization order for both evaluation request shapes.
fn prepare_context(
    input: &Path,
    base: &Path,
    output: Option<&Path>,
    context_file: &Path,
    isolation: &mut HoldoutIsolation,
    upstream: &RemoteSnapshot,
    skills: &mut VerificationArtifact,
) -> Result<EvaluationContext> {
    resolve_isolation(base, isolation);
    let context_path = relative_to(base, context_file);
    check_gate_paths(input, &context_path, output, isolation)?;
    let context: EvaluationContext = read_strict(&context_path)?;
    validate_evaluation_baseline(&context, upstream)?;
    prepare_skills_artifact(base, skills, isolation)?;
    Ok(context)
}

fn emit_receipt(receipt: EvaluationReceipt, output: Option<&Path>) -> Result<Value> {
    receipt.validate()?;
    write_optional(output, &receipt)?;
    Ok(json!({"kind":receipt.kind,"decision":receipt.decision,"receipt_file":output}))
}

pub(super) fn promotion(input: &Path, base: &Path, output: Option<PathBuf>) -> Result<Value> {
    let mut request: PromotionRequest = read_strict(input)?;
    let context = prepare_context(
        input,
        base,
        output.as_deref(),
        &request.context_file,
        &mut request.holdout_isolation,
        &request.upstream,
        &mut request.skills_manifest,
    )?;
    canonicalize_observations(&mut request.observations);
    let mut decision = evaluate_promotion(&context, &request.observations)?;
    bind_upstream_digest(&mut decision, &request.upstream, &request.skills_manifest)?;
    let receipt = EvaluationReceipt {
        schema_version: 2,
        upstream: Some(request.upstream),
        skills_manifest: Some(request.skills_manifest),
        kind: EvaluationKind::Promotion,
        context,
        observations: request.observations,
        ablations: vec![],
        decision,
    };
    emit_receipt(receipt, output.as_deref())
}

pub(super) fn novelty(input: &Path, base: &Path, output: Option<PathBuf>) -> Result<Value> {
    let mut request: NoveltyRequest = read_strict(input)?;
    let context = prepare_context(
        input,
        base,
        output.as_deref(),
        &request.context_file,
        &mut request.holdout_isolation,
        &request.upstream,
        &mut request.skills_manifest,
    )?;
    canonicalize_observations(&mut request.observations);
    canonicalize_ablations(&mut request.ablations);
    let mut decision =
        evaluate_novel_capability(&context, &request.observations, &request.ablations)?;
    bind_upstream_digest(&mut decision, &request.upstream, &request.skills_manifest)?;
    let receipt = EvaluationReceipt {
        schema_version: 2,
        upstream: Some(request.upstream),
        skills_manifest: Some(request.skills_manifest),
        kind: EvaluationKind::Novelty,
        context,
        observations: request.observations,
        ablations: request.ablations,
        decision,
    };
    emit_receipt(receipt, output.as_deref())
}

#[cfg(test)]
#[path = "evaluation_tests.rs"]
mod tests;
