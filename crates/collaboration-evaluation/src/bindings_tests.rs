use super::*;
use crate::tests::{card, fixture, job_and_receipt};
use serde_json::json;

#[test]
fn receipt_authority_and_frozen_model_reject_local_tampering() {
    let (_temp, c, plan, citation) = fixture();
    let entry = &plan.tasks[0];
    let (job, receipt, state) = job_and_receipt(&c, &plan, entry, &card(&citation));
    check_receipt(&c, &plan, entry, &job, &state, &receipt).unwrap();
    let mut altered = receipt.clone();
    altered["agent_report"]["summary"] = json!("forged");
    assert!(check_receipt(&c, &plan, entry, &job, &state, &altered).is_err());
    let mut changed = job.clone();
    changed.model = "another model".into();
    assert!(check_receipt(&c, &plan, entry, &changed, &state, &receipt).is_err());
    changed = job;
    changed.attempt = 2;
    assert!(check_receipt(&c, &plan, entry, &changed, &state, &receipt).is_err());
}
