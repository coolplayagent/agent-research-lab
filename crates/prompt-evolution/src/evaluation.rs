//! Advisory promotion and novelty gates over validated paired observations.
use crate::{
    identity::{digest, nonempty},
    metrics::{improved_efficiency, summarize},
    pairing::{critical_regression, paired_trials},
    protocol::{AblationStudy, EvaluationContext, EvaluationDecision, TrialObservation},
};
use anyhow::{Result, ensure};
use std::collections::BTreeSet;

/// Malformed, duplicated, unpaired or stale evidence is an error. Valid but
/// insufficient or unsuccessful evidence produces a denied decision.
pub fn evaluate_promotion(
    context: &EvaluationContext,
    observations: &[TrialObservation],
) -> Result<EvaluationDecision> {
    let families = paired_trials(context, observations)?;
    let metrics = summarize(&families);
    let mut reasons = Vec::new();
    if families.len() < 3 || families.values().any(|pairs| pairs.len() < 3) {
        reasons.push("require at least three task families with three paired trials each".into());
    }
    if critical_regression(&families) {
        reasons.push("candidate has a critical regression".into());
    }
    let success_improved = metrics.candidate_successes > metrics.baseline_successes
        && (metrics.candidate_successes - metrics.baseline_successes) as u128 * 10
            >= metrics.pairs as u128;
    if !success_improved && !improved_efficiency(&metrics) {
        reasons.push("require +10 percentage points success or nondecreasing success with 10% median cost improvement".into());
    }
    Ok(EvaluationDecision {
        approved: reasons.is_empty(),
        reasons,
        metrics,
        evidence_digest: digest(&(context, observations))?,
    })
}

/// Novelty requires held-out-family transfer and a measured ablation. This
/// records reproducible evidence, not a claim of statistical significance.
pub fn evaluate_novel_capability(
    context: &EvaluationContext,
    observations: &[TrialObservation],
    ablations: &[AblationStudy],
) -> Result<EvaluationDecision> {
    let families = paired_trials(context, observations)?;
    let metrics = summarize(&families);
    let mut reasons = Vec::new();
    if families.len() < 2 || families.values().any(|pairs| pairs.len() != 3) {
        reasons.push("novel capability requires exactly three paired trials in each of at least two held-out families".into());
    }
    if families.values().any(|pairs| {
        pairs.iter().filter(|p| p.candidate.success).count() < 2
            || pairs.iter().filter(|p| p.baseline.success).count() > 1
    }) {
        reasons
            .push("each family requires candidate success >= 2 and baseline success <= 1".into());
    }
    if critical_regression(&families) {
        reasons.push("candidate has a critical regression".into());
    }
    let mut components = BTreeSet::new();
    let mut demonstrated_contribution = false;
    for ablation in ablations {
        nonempty(&ablation.removed_component, "ablated component")?;
        ensure!(
            components.insert(&ablation.removed_component),
            "duplicate ablation component"
        );
        let ablation_context = EvaluationContext {
            baseline: ablation.ablated_snapshot.clone(),
            candidate: context.candidate.clone(),
        };
        let ablated_families = paired_trials(&ablation_context, &ablation.observations)?;
        ensure!(
            ablated_families
                .keys()
                .all(|family| families.contains_key(family)),
            "ablation must test the same held-out task families"
        );
        let summary = summarize(&ablated_families);
        if summary.pairs >= 3
            && !critical_regression(&ablated_families)
            && (summary.candidate_successes > summary.baseline_successes
                || improved_efficiency(&summary))
        {
            demonstrated_contribution = true;
        }
    }
    if !demonstrated_contribution {
        reasons.push("require a paired ablation demonstrating a component's contribution with at least three trials".into());
    }
    Ok(EvaluationDecision {
        approved: reasons.is_empty(),
        reasons,
        metrics,
        evidence_digest: digest(&(context, observations, ablations))?,
    })
}

#[cfg(test)]
#[path = "evaluation_tests.rs"]
mod tests;
