use super::*;
use crate::tests::admission;
use collaboration_model::{EvolutionAction, EvolutionChange};
use std::fs;

#[test]
fn switch_marker_requires_a_readable_admission_intent() {
    let dir = tempfile::tempdir().unwrap();
    let operation = dir.path();
    let path = operation.join("admission.json");
    let marker = operation.join("switch-started.json");
    assert_eq!(activation_claim(operation).unwrap(), None);
    assert!(mark_switch(operation).is_err());
    assert!(!marker.exists());
    for action in [EvolutionAction::Pause, EvolutionAction::Stop] {
        storage::write(
            &path,
            &EvolutionChange {
                request_id: "other-command".into(),
                run_id: "iteration".into(),
                revision: 7,
                action,
            },
        )
        .unwrap();
        assert!(activation_claim(operation).is_err());
        assert!(mark_switch(operation).is_err());
        assert!(!marker.exists());
    }
    fs::write(&path, b"{truncated").unwrap();
    assert!(activation_claim(operation).is_err());
    assert!(mark_switch(operation).is_err());
    assert!(!marker.exists());
    admission(operation);
    let bytes = fs::read(&path).unwrap();
    mark_switch(operation).unwrap();
    let recorded: Value = storage::read(&marker).unwrap();
    assert_eq!(recorded["activation_claim_id"], "activation-claim");
    assert!(recorded["started_ms"].as_u64().unwrap() > 0);
    assert_eq!(fs::read(path).unwrap(), bytes);
}

#[test]
fn terminal_receipt_requires_claim_and_gate_bytes_and_preserves_bindings() {
    let dir = tempfile::tempdir().unwrap();
    let operation = dir.path();
    let prepared = crate::tests::prepared(operation);
    let artifact = operation.join("artifact");
    let digest = "9".repeat(64);
    assert!(bound_receipt(operation, &prepared, true, &artifact, &digest).is_err());
    admission(operation);
    assert!(bound_receipt(operation, &prepared, true, &artifact, &digest).is_err());
    let gate = b"{\"verified\":\"fixture\"}\n";
    fs::write(operation.join("merged-qualitygate.json"), gate).unwrap();
    for (active, phase) in [
        (true, ActivationPhase::Active),
        (false, ActivationPhase::RolledBack),
    ] {
        let receipt = bound_receipt(operation, &prepared, active, &artifact, &digest).unwrap();
        assert_eq!(
            receipt,
            json!({
                "schema_version": 1, "goal_id": "goal",
                "candidate_commit": "a".repeat(40), "merged_commit": "b".repeat(40),
                "active": active, "phase": phase, "activation_claim_id": "activation-claim",
                "artifact_sha256": digest, "reference": artifact,
                "policy_sha256": "2".repeat(64), "qualitygate_sha256": storage::digest(gate),
            })
        );
    }
    assert!(!operation.join("receipt.json").exists());
}
