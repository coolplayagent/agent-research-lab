//! Recomputable evidence, canonical ordering and source/skills digest bindings.
use anyhow::{Context, Result, ensure};
pub use contracts::PromptEvaluationKind as EvaluationKind;
use delivery::VerificationArtifact;
use evolution::{
    AblationStudy, EvaluationContext, EvaluationDecision, TrialObservation, Variant,
    evaluate_novel_capability, evaluate_promotion,
};
use freshness::RemoteSnapshot;
use serde::{Deserialize, Serialize};

/// Full host receipt enables downstream consumers to recompute the gate instead
/// of trusting an `approved` field, including after a controller restart.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvaluationReceipt {
    pub schema_version: u32,
    /// Missing only on historical receipts, which remain readable but cannot
    /// authorize new promotion or exploration-budget increases.
    #[serde(default)]
    pub upstream: Option<RemoteSnapshot>,
    #[serde(default)]
    pub skills_manifest: Option<VerificationArtifact>,
    pub kind: EvaluationKind,
    pub context: EvaluationContext,
    pub observations: Vec<TrialObservation>,
    pub ablations: Vec<AblationStudy>,
    pub decision: EvaluationDecision,
}

impl EvaluationReceipt {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == 2,
            "unsupported evaluation receipt version"
        );
        let upstream = self.upstream.as_ref().context(
            "old evaluation receipt lacks an upstream baseline; re-evaluation is required",
        )?;
        validate_evaluation_baseline(&self.context, upstream)?;
        let skills = self.skills_manifest.as_ref().context(
            "old evaluation receipt lacks a hashed skills manifest; re-evaluation is required",
        )?;
        skills.validate_skills()?;
        let mut canonical = self.observations.clone();
        canonicalize_observations(&mut canonical);
        ensure!(
            serde_json::to_value(&canonical)? == serde_json::to_value(&self.observations)?,
            "evaluation observations are not in canonical order"
        );
        let mut ablations = self.ablations.clone();
        canonicalize_ablations(&mut ablations);
        ensure!(
            serde_json::to_value(&ablations)? == serde_json::to_value(&self.ablations)?,
            "ablation evidence is not in canonical order"
        );
        let mut recomputed = match self.kind {
            EvaluationKind::Promotion => {
                ensure!(
                    self.ablations.is_empty(),
                    "promotion receipt cannot contain ablations"
                );
                evaluate_promotion(&self.context, &self.observations)?
            }
            EvaluationKind::Novelty => {
                evaluate_novel_capability(&self.context, &self.observations, &self.ablations)?
            }
        };
        bind_upstream_digest(&mut recomputed, upstream, skills)?;
        ensure!(
            serde_json::to_value(&recomputed)? == serde_json::to_value(&self.decision)?,
            "evaluation receipt does not match recomputed evidence"
        );
        Ok(())
    }

    pub fn critical_regression(&self) -> bool {
        self.observations
            .iter()
            .any(|trial| trial.variant == Variant::Candidate && trial.critical_regression)
            || self
                .ablations
                .iter()
                .flat_map(|study| &study.observations)
                .any(|trial| trial.variant == Variant::Candidate && trial.critical_regression)
    }
}

pub(super) fn validate_evaluation_baseline(
    context: &EvaluationContext,
    upstream: &RemoteSnapshot,
) -> Result<()> {
    upstream.validate()?;
    ensure!(
        context.baseline.source_commit == upstream.commit,
        "evaluation source differs from the captured upstream default baseline"
    );
    Ok(())
}

pub(super) fn bind_upstream_digest(
    decision: &mut EvaluationDecision,
    upstream: &RemoteSnapshot,
    skills: &VerificationArtifact,
) -> Result<()> {
    decision.evidence_digest = storage::digest(&serde_json::to_vec(&(
        &decision.evidence_digest,
        &upstream.repository,
        &upstream.default_branch,
        &upstream.commit,
        &skills.sha256,
    ))?);
    Ok(())
}

pub(super) fn canonicalize_observations(observations: &mut [TrialObservation]) {
    observations.sort_by(|a, b| {
        (&a.task_family, &a.trial_id, a.variant == Variant::Candidate).cmp(&(
            &b.task_family,
            &b.trial_id,
            b.variant == Variant::Candidate,
        ))
    });
}

pub(super) fn canonicalize_ablations(ablations: &mut [AblationStudy]) {
    ablations.sort_by(|a, b| a.removed_component.cmp(&b.removed_component));
    for ablation in ablations {
        canonicalize_observations(&mut ablation.observations);
    }
}

#[cfg(test)]
#[path = "receipts_tests.rs"]
mod tests;
