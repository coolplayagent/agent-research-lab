//! Apply independent host grades only to their exact plan, claim and source bindings.
use crate::{
    citations::check_citation,
    protocol::{Adjudications, Change, CollectedClaim, Plan},
};
use anyhow::{Context, Result, ensure};
use std::collections::BTreeSet;

pub(super) fn apply_adjudications(
    plan: &Plan,
    claims: &mut [CollectedClaim],
    grades: &Adjudications,
) -> Result<()> {
    ensure!(
        grades.schema_version == 1 && grades.plan_digest == plan.digest,
        "adjudications bind another plan"
    );
    let mut seen = BTreeSet::new();
    for grade in &grades.entries {
        ensure!(seen.insert(&grade.claim_id), "duplicate adjudication");
        let index = claims
            .iter()
            .position(|c| c.id == grade.claim_id)
            .context("grade references an absent claim")?;
        let claim = &claims[index];
        ensure!(
            grade.claim_digest == claim.digest,
            "grade binds a stale claim or receipt"
        );
        ensure!(
            !grade.rationale.trim().is_empty() && grade.rationale.len() <= 4096,
            "independent rationale required"
        );
        ensure!(
            !grade.evidence.is_empty() && grade.evidence.len() <= 8,
            "host judgment requires bounded independent citations"
        );
        for citation in &grade.evidence {
            check_citation(plan, citation)?;
        }
        ensure!(
            !grade.supported || claim.citation_errors.is_empty(),
            "cannot approve a claim with invalid references"
        );
        ensure!(
            !grade.effective_counterexample || !claim.card.counterexample_to.is_empty(),
            "effective counterexample lacks a target"
        );
        ensure!(
            !grade.valid_revision
                || (!claim.card.revises.is_empty()
                    && matches!(claim.card.change, Change::Narrow | Change::Retract)),
            "valid revision lacks a changed prior claim"
        );
        ensure!(
            grade.introduced_error.is_none() || claim.round == 2,
            "error-change judgment only applies to final claims"
        );
        ensure!(
            grade.introduced_error != Some(true) || !grade.supported,
            "a supported claim cannot introduce an error"
        );
        if let Some(id) = &grade.duplicate_of {
            let representative = claims
                .iter()
                .find(|c| &c.id == id)
                .context("duplicate representative missing")?;
            ensure!(
                id < &claim.id
                    && representative.arm == claim.arm
                    && representative.round == claim.round,
                "duplicate representative must precede claim in same arm/round"
            );
        }
        claims[index].adjudication = Some(grade.clone());
    }
    Ok(())
}

#[cfg(test)]
#[path = "adjudication_tests.rs"]
mod tests;
