//! Validate exact paired holdout evidence before summary or gate evaluation.
use crate::{
    identity::nonempty,
    protocol::{EvaluationContext, TrialObservation},
};
use anyhow::{Context, Result, ensure};
use contracts::{DatasetPartition, TrialVariant as Variant};
use std::collections::BTreeMap;

pub(super) struct Pair<'a> {
    pub(super) baseline: &'a TrialObservation,
    pub(super) candidate: &'a TrialObservation,
}

pub(super) type Families<'a> = BTreeMap<&'a str, Vec<Pair<'a>>>;

pub(super) fn paired_trials<'a>(
    context: &EvaluationContext,
    observations: &'a [TrialObservation],
) -> Result<Families<'a>> {
    context.validate()?;
    let mut paired =
        BTreeMap::<(&str, &str), (Option<&TrialObservation>, Option<&TrialObservation>)>::new();
    let mut trial_families = BTreeMap::new();
    for observation in observations {
        nonempty(&observation.trial_id, "trial ID")?;
        nonempty(&observation.task_family, "task family")?;
        ensure!(
            observation.partition == DatasetPartition::Holdout,
            "promotion evidence must come from held-out tasks"
        );
        if let Some(family) = trial_families.insert(&observation.trial_id, &observation.task_family)
        {
            ensure!(
                family == &observation.task_family,
                "trial ID reused across task families"
            );
        }
        let expected = match observation.variant {
            Variant::Baseline => &context.baseline,
            Variant::Candidate => &context.candidate,
        };
        ensure!(
            &observation.snapshot == expected,
            "stale or mismatched evidence snapshot"
        );
        let entry = paired
            .entry((&observation.task_family, &observation.trial_id))
            .or_default();
        let slot = match observation.variant {
            Variant::Baseline => &mut entry.0,
            Variant::Candidate => &mut entry.1,
        };
        ensure!(slot.is_none(), "duplicate trial observation");
        *slot = Some(observation);
    }
    let mut families = BTreeMap::<&str, Vec<Pair<'a>>>::new();
    for ((family, _), (baseline, candidate)) in paired {
        families.entry(family).or_default().push(Pair {
            baseline: baseline.context("missing baseline trial partner")?,
            candidate: candidate.context("missing candidate trial partner")?,
        });
    }
    Ok(families)
}

pub(super) fn critical_regression(families: &Families<'_>) -> bool {
    families
        .values()
        .flatten()
        .any(|pair| pair.candidate.critical_regression)
}

#[cfg(test)]
#[path = "pairing_tests.rs"]
mod tests;
