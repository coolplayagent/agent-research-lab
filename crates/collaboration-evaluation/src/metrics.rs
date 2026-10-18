//! Pure team-level summaries keep worker proposals separate from host judgments.
use crate::protocol::{Arm, Change, CollectedClaim, Plan};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Default, Serialize)]
pub(super) struct Metrics {
    pub(super) scheduled_calls: usize,
    pub(super) succeeded_calls: usize,
    pub(super) failed_calls: usize,
    pub(super) incomplete_calls: usize,
    pub(super) invalid_reports: usize,
    final_claims: usize,
    resolvable_citations: usize,
    invalid_citations: usize,
    literal_duplicate_claims: usize,
    proposed_counterexamples: usize,
    proposed_revisions: usize,
    adjudicated_claims: usize,
    supported_claims: usize,
    unsupported_claims: usize,
    effective_counterexamples: usize,
    valid_revisions: usize,
    unique_counterexample_targets: usize,
    revision_error_comparisons: usize,
    introduced_errors: usize,
    semantic_duplicates: usize,
    unique_supported_claims: usize,
}

pub(super) fn summarize(
    plan: &Plan,
    mut metrics: BTreeMap<Arm, Metrics>,
    claims: Vec<CollectedClaim>,
    tasks: Vec<Value>,
) -> Value {
    let mut fingerprints = BTreeSet::new();
    let mut supported_fingerprints = BTreeSet::new();
    let mut corrected_targets = BTreeSet::new();
    for claim in claims.iter().filter(|c| c.round == 2) {
        let m = metrics.get_mut(&claim.arm).unwrap();
        m.final_claims += 1;
        m.invalid_citations += claim.citation_errors.len();
        m.resolvable_citations += claim.card.evidence.len() - claim.citation_errors.len();
        let normalized = claim
            .card
            .text
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();
        if !fingerprints.insert((claim.arm, normalized.clone())) {
            m.literal_duplicate_claims += 1;
        }
        m.proposed_counterexamples += usize::from(!claim.card.counterexample_to.is_empty());
        m.proposed_revisions += usize::from(matches!(
            claim.card.change,
            Change::Narrow | Change::Retract
        ));
        if let Some(g) = &claim.adjudication {
            m.adjudicated_claims += 1;
            m.supported_claims += usize::from(g.supported);
            m.unsupported_claims += usize::from(!g.supported);
            m.effective_counterexamples += usize::from(g.effective_counterexample);
            m.valid_revisions += usize::from(g.valid_revision);
            if g.effective_counterexample {
                for target in &claim.card.counterexample_to {
                    m.unique_counterexample_targets +=
                        usize::from(corrected_targets.insert((claim.arm, target)));
                }
            }
            m.revision_error_comparisons += usize::from(g.introduced_error.is_some());
            m.introduced_errors += usize::from(g.introduced_error == Some(true));
            m.semantic_duplicates += usize::from(g.duplicate_of.is_some());
            m.unique_supported_claims += usize::from(
                g.supported
                    && g.duplicate_of.is_none()
                    && supported_fingerprints.insert((claim.arm, normalized)),
            );
        }
    }
    let complete = metrics.values().all(|m| m.incomplete_calls == 0);
    let knowledge_proposals:Vec<_>=claims.iter().filter_map(|claim|claim.card.knowledge_update.as_ref().map(|update|json!({"status":contracts::ProposalState::PendingHostReview,"superpod_commit":plan.inputs.repositories["superpod"].upstream.commit,"claim_id":claim.id,"claim_digest":claim.digest,"target_path":update.target_path,"proposal":update.proposal}))).collect();
    json!({"schema_version":1,"protocol":plan.protocol,"plan_digest":plan.digest,"team_pairs":1,"promotion_eligible":false,"call_unit":"planned_codex_exec_attempt",
        "complete":complete,"fully_adjudicated":complete && !claims.is_empty() && metrics.values().all(|m|m.invalid_reports==0) && claims.iter().all(|c|c.adjudication.is_some()),
        "metrics":metrics,"tasks":tasks,"claims":claims,"knowledge_proposals":knowledge_proposals,
        "limitations":["One team-level pilot; coupled slots are not independent trials.","Equal call/time ceilings do not imply equal actual tokens or cost.","Resolvable citations do not prove semantic support; host adjudications are separate.","Literal repeated claims are not measured duplicate work; semantic duplication requires host adjudication.","Worker-writable trace usage is diagnostic, not independent billing or promotion evidence.","Failures remain in the planned denominator. No automatic TrialObservation, promotion, policy change or knowledge publication."]})
}

#[cfg(test)]
#[path = "metrics_tests.rs"]
mod tests;
