use super::*;

#[test]
fn unadmitted_reconciliation_releases_drain_without_claiming_installation() {
    let dir = tempfile::tempdir().unwrap();
    let operation = fs::canonicalize(dir.path()).unwrap();
    let prepared = crate::tests::prepared(&operation);
    storage::write(&operation.join("prepared.json"), &prepared).unwrap();
    let marker = prepared.state_dir.join("private/activation-drain");
    storage::write(&marker, &json!({"operation":operation})).unwrap();
    let receipt = crate::reconcile(&operation).unwrap();
    assert_eq!(receipt["goal_id"], "goal");
    assert_eq!(receipt["candidate_commit"], "a".repeat(40));
    assert_eq!(receipt["merged_commit"], "b".repeat(40));
    assert_eq!(receipt["active"], false);
    assert_eq!(
        ActivationPhase::from_value(&receipt["phase"]),
        Some(ActivationPhase::Blocked)
    );
    assert_eq!(receipt["requires_reconciliation"], false);
    assert!(receipt.get("activation_claim_id").is_none());
    assert!(!marker.exists());
    assert!(!operation.join("receipt.json").exists());
    assert!(!operation.join("switch-started.json").exists());
    assert!(!operation.join("source").exists());
    assert!(!prepared.policy.service_config.exists());
}
