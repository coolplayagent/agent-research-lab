use super::*;
use crate::{
    cards::report_cards,
    protocol::{Adjudication, Change},
    tests::{card, fixture, job_and_receipt},
};

#[test]
fn cross_arm_references_and_stale_host_grades_are_rejected() {
    let (_temp, c, plan, citation) = fixture();
    let entry = &plan.tasks[24];
    let mut proposed = card(&citation);
    proposed.change = Change::Narrow;
    proposed.revises = vec![format!("{}#claim-one", plan.tasks[0].task.id)];
    let (_, receipt, _) = job_and_receipt(&c, &plan, entry, &proposed);
    let known = BTreeSet::from([proposed.revises[0].clone()]);
    assert!(report_cards(entry, &receipt, 3, &known).is_err());
    let initial = &plan.tasks[0];
    let (_, receipt, _) = job_and_receipt(&c, &plan, initial, &card(&citation));
    let mut claims = report_cards(initial, &receipt, 3, &BTreeSet::new()).unwrap();
    let mut grades = Adjudications {
        schema_version: 1,
        plan_digest: plan.digest.clone(),
        entries: vec![Adjudication {
            claim_id: claims[0].id.clone(),
            claim_digest: "stale".into(),
            supported: true,
            effective_counterexample: false,
            valid_revision: false,
            introduced_error: None,
            duplicate_of: None,
            rationale: "Independently checked exact source".into(),
            evidence: vec![citation],
        }],
    };
    assert!(apply_adjudications(&plan, &mut claims, &grades).is_err());
    grades.entries[0].claim_digest = claims[0].digest.clone();
    apply_adjudications(&plan, &mut claims, &grades).unwrap();
    assert!(claims[0].adjudication.as_ref().unwrap().supported);
    grades.entries[0].valid_revision = true;
    assert!(apply_adjudications(&plan, &mut claims, &grades).is_err());
}
