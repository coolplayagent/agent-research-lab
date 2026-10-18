use super::*;

#[test]
fn early_failure_releases_unclaimed_drain_but_preserves_uncertain_admission() {
    for has_admission in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let operation = fs::canonicalize(dir.path()).unwrap();
        let prepared = crate::tests::prepared(&operation);
        storage::write(&operation.join("prepared.json"), &prepared).unwrap();
        phase(&operation, ActivationPhase::Prepared, json!({})).unwrap();
        let marker = prepared.state_dir.join("private/activation-drain");
        storage::write(&marker, &json!({"operation":operation})).unwrap();
        let original = fs::read(&marker).unwrap();
        if has_admission {
            crate::tests::admission(&operation);
        }

        // The absent config fails before build or service commands. An absent storage
        // endpoint makes a journaled admission uncertain, so the drain must remain.
        assert!(!prepared.policy.service_config.exists());
        assert!(crate::worker(&operation).is_err());
        let status: Value = storage::read(&operation.join("status.json")).unwrap();
        assert_eq!(
            ActivationPhase::from_value(&status["phase"]),
            Some(ActivationPhase::Blocked)
        );
        assert!(status["details"]["error"].as_str().is_some());
        assert_eq!(marker.exists(), has_admission);
        if has_admission {
            assert_eq!(fs::read(marker).unwrap(), original);
        }
        assert!(!operation.join("receipt.json").exists());
        assert!(!operation.join("switch-started.json").exists());
        assert!(!operation.join("source").exists());
        assert!(!prepared.policy.service_config.exists());
    }
}
