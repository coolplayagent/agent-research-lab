use super::*;
use crate::{
    protocol::ReviewRecord,
    tests::{evidence, upstream},
};
use std::path::PathBuf;

#[test]
fn merge_rejects_stale_unknown_failed_and_unresolved_evidence() {
    let temp = tempfile::tempdir().unwrap();
    let request = MergeRequest {
        repository: "coolplayagent/example".into(),
        working_directory: PathBuf::from("."),
        number: 1,
        expected_head: "a".repeat(40),
        evidence: evidence(temp.path()),
    };
    let valid = MergeSnapshot {
        head: request.expected_head.clone(),
        base_ref: "main".into(),
        base_commit: upstream().commit.clone(),
        default_branch: "main".into(),
        default_commit: upstream().commit,
        state: "OPEN".into(),
        draft: false,
        author: "author".into(),
        mergeable: "MERGEABLE".into(),
        merge_state: "CLEAN".into(),
        review_decision: Some("APPROVED".into()),
        checks: vec!["SUCCESS".into()],
        reviews: vec![ReviewRecord {
            author: "reviewer".into(),
            state: "APPROVED".into(),
            commit: request.expected_head.clone(),
        }],
        unresolved_threads: 0,
        complete: true,
    };
    evaluate_merge_gate(&request, &valid).unwrap();
    let mut stale = valid.clone();
    stale.head = "e".repeat(40);
    assert!(evaluate_merge_gate(&request, &stale).is_err());
    let mut advanced = valid.clone();
    advanced.base_commit = "1".repeat(40);
    advanced.default_commit = advanced.base_commit.clone();
    assert!(evaluate_merge_gate(&request, &advanced).is_err());
    let mut retargeted = valid.clone();
    retargeted.base_ref = "old-release".into();
    assert!(evaluate_merge_gate(&request, &retargeted).is_err());
    let mut incomplete = valid.clone();
    incomplete.complete = false;
    assert!(evaluate_merge_gate(&request, &incomplete).is_err());
    let mut pending = valid.clone();
    pending.checks = vec!["PENDING".into()];
    assert!(evaluate_merge_gate(&request, &pending).is_err());
    let mut unresolved = valid.clone();
    unresolved.unresolved_threads = 1;
    assert!(evaluate_merge_gate(&request, &unresolved).is_err());
    let mut missing_review = valid.clone();
    missing_review.review_decision = Some("REVIEW_REQUIRED".into());
    assert!(evaluate_merge_gate(&request, &missing_review).is_err());
    let mut no_required_review = valid;
    no_required_review.reviews.clear();
    no_required_review.review_decision = None;
    evaluate_merge_gate(&request, &no_required_review).unwrap();
}
