use super::*;
use crate::{
    protocol::{Adjudication, Citation},
    tests::{card, fixture},
};

fn claim(citation: &Citation, arm: Arm, id: &str, assessed: bool) -> CollectedClaim {
    let mut card = card(citation);
    card.text = "Recovery   requires observability.".into();
    card.change = Change::Narrow;
    card.revises = vec![format!("{arm:?}-initial#claim")];
    card.counterexample_to = card.revises.clone();
    let digest = format!("digest-{id}");
    let adjudication = assessed.then(|| Adjudication {
        claim_id: id.into(),
        claim_digest: digest.clone(),
        supported: true,
        effective_counterexample: true,
        valid_revision: true,
        introduced_error: Some(false),
        duplicate_of: None,
        rationale: "Independent source comparison".into(),
        evidence: vec![citation.clone()],
    });
    CollectedClaim {
        id: id.into(),
        digest,
        arm,
        round: 2,
        card,
        citation_errors: vec![],
        adjudication,
    }
}

#[test]
fn final_metrics_distinguish_host_judgments_and_deduplicate_within_each_arm() {
    let (_temp, _c, plan, citation) = fixture();
    let mut initial = claim(&citation, Arm::Baseline, "initial", true);
    initial.round = 1;
    let first = claim(&citation, Arm::Baseline, "first", true);
    let mut duplicate = claim(&citation, Arm::Baseline, "second", true);
    duplicate.card.text = "RECOVERY requires observability.".into();
    duplicate.adjudication.as_mut().unwrap().duplicate_of = Some("first".into());
    let unassessed = claim(&citation, Arm::Baseline, "third", false);
    let other_arm = claim(&citation, Arm::Candidate, "candidate", true);
    let metrics = BTreeMap::from([
        (
            Arm::Baseline,
            Metrics {
                scheduled_calls: 16,
                succeeded_calls: 16,
                ..Metrics::default()
            },
        ),
        (
            Arm::Candidate,
            Metrics {
                scheduled_calls: 16,
                succeeded_calls: 16,
                ..Metrics::default()
            },
        ),
    ]);
    let result = summarize(
        &plan,
        metrics,
        vec![initial, first, duplicate, unassessed, other_arm],
        vec![],
    );
    let baseline = &result["metrics"]["baseline"];
    for (field, count) in [
        ("scheduled_calls", 16),
        ("final_claims", 3),
        ("literal_duplicate_claims", 2),
        ("proposed_counterexamples", 3),
        ("proposed_revisions", 3),
        ("adjudicated_claims", 2),
        ("supported_claims", 2),
        ("effective_counterexamples", 2),
        ("valid_revisions", 2),
        ("unique_counterexample_targets", 1),
        ("revision_error_comparisons", 2),
        ("introduced_errors", 0),
        ("semantic_duplicates", 1),
        ("unique_supported_claims", 1),
    ] {
        assert_eq!(baseline[field], count, "wrong {field}");
    }
    let candidate = &result["metrics"]["candidate"];
    assert_eq!(candidate["final_claims"], 1);
    assert_eq!(candidate["literal_duplicate_claims"], 0);
    assert_eq!(candidate["unique_supported_claims"], 1);
    assert_eq!(candidate["unique_counterexample_targets"], 1);
    assert_eq!(result["complete"], true);
    assert_eq!(result["fully_adjudicated"], false);
    assert_eq!(result["promotion_eligible"], false);
    assert_eq!(result["team_pairs"], 1);
    assert!(
        result["knowledge_proposals"]
            .as_array()
            .unwrap()
            .iter()
            .all(|proposal| {
                proposal["status"] == contracts::ProposalState::PendingHostReview.as_str()
            })
    );
}
