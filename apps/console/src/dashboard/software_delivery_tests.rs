use super::*;
#[test]
fn candidate_snapshot_budget_preserves_legacy_policy_and_full_gate_arguments() {
    let legacy = r#"{"schema_version":1,"repositories":{"example":{"repository":"owner/repo","tests":[["cargo","test"]],"activation":["host","activate"]}}}"#;
    let policy: DeliveryPolicy = serde_json::from_str(legacy).unwrap();
    let bytes = serde_json::to_vec(&policy).unwrap();
    assert_eq!(bytes, legacy.as_bytes());
    assert_eq!(
        storage::digest(&bytes),
        "760d52ac5569c44711a14013d1dff8aa3f2f4dd36d8242933f48b50471b39d8e"
    );
    let expected = [
        "qualitygate",
        "check",
        "--root",
        "/candidate",
        "--diff",
        "baseline..candidate",
        "--profile",
        "full",
        "--format",
        "json",
    ]
    .map(str::to_owned)
    .to_vec();
    assert_eq!(
        policy.repositories["example"].qualitygate_command(
            "qualitygate".into(),
            Path::new("/candidate"),
            "baseline",
            "candidate",
        ),
        expected
    );
    for budget in [1, 5, 8] {
        let mut value: Value = serde_json::from_str(legacy).unwrap();
        value["repositories"]["example"]["qualitygate_snapshot_max_file_mib"] = json!(budget);
        let configured: DeliveryPolicy = serde_json::from_value(value).unwrap();
        let args = configured.repositories["example"].qualitygate_command(
            "qualitygate".into(),
            Path::new("/candidate"),
            "baseline",
            "candidate",
        );
        assert_eq!(args[..expected.len()], expected);
        assert_eq!(
            args[expected.len()..],
            ["--snapshot-max-file-mib".to_owned(), budget.to_string()]
        );
        assert_ne!(
            storage::digest(&serde_json::to_vec(&configured).unwrap()),
            storage::digest(&bytes)
        );
        assert_eq!(
            serde_json::to_value(&configured).unwrap()["repositories"]["example"]["qualitygate_snapshot_max_file_mib"],
            budget
        );
    }
    for invalid in [
        json!(0),
        json!(9),
        json!(-1),
        json!(256),
        json!(1.5),
        json!("5"),
        Value::Null,
    ] {
        let mut value: Value = serde_json::from_str(legacy).unwrap();
        value["repositories"]["example"]["qualitygate_snapshot_max_file_mib"] = invalid;
        assert!(serde_json::from_value::<DeliveryPolicy>(value).is_err());
    }
}
#[test]
fn activation_adapters_decode_native_cli_envelopes_and_raw_receipts() {
    let receipt = json!({"schema_version":1,"active":true,"goal_id":"goal",
            "candidate_commit":"a".repeat(40),"merged_commit":"b".repeat(40),
            "artifact_sha256":"c".repeat(64),"reference":"artifact/version", "activation_claim_id":"claim"});
    for encoded in [receipt.clone(), json!({"ok":true,"result":receipt})] {
        let decoded = activation_output(&serde_json::to_vec(&encoded).unwrap()).unwrap();
        assert_eq!(decoded, receipt);
        validate_activation(&decoded, &"a".repeat(40), &"b".repeat(40), "goal").unwrap();
    }
    for malformed in [
        json!({"ok":true,"error":"failed","result":receipt}),
        json!({"ok":true,"active":false,"result":receipt}),
        json!({"ok":false,"result":receipt}),
        json!({"ok":"true","result":receipt}),
        json!({"ok":true}),
        json!({"ok":true,"result":null}),
        json!({"schema_version":2,"active":true}),
    ] {
        assert!(activation_output(&serde_json::to_vec(&malformed).unwrap()).is_err());
    }
    let pending =
        json!({"schema_version":1,"active":false,"phase":contracts::ActivationPhase::Prepared});
    let decoded =
        activation_output(&serde_json::to_vec(&json!({"ok":true,"result":pending})).unwrap())
            .unwrap();
    assert_eq!(decoded, pending);
    assert!(validate_activation(&decoded, &"a".repeat(40), &"b".repeat(40), "goal").is_err());
}
#[test]
fn activation_requires_actual_merged_source_and_current_goal() {
    let mut receipt = json!({"schema_version":1,"active":true,"candidate_commit":"a".repeat(40),"merged_commit":"b".repeat(40),"goal_id":"goal","artifact_sha256":"c".repeat(64),"reference":"artifact/version"});
    assert!(validate_activation(&receipt, &"a".repeat(40), &"b".repeat(40), "goal").is_ok());
    assert!(validate_activation(&receipt, &"a".repeat(40), &"d".repeat(40), "goal").is_err());
    assert!(
        validate_activation(&receipt, &"a".repeat(40), &"b".repeat(40), "another-goal").is_err()
    );
    receipt["active"] = json!(false);
    assert!(validate_activation(&receipt, &"a".repeat(40), &"b".repeat(40), "goal").is_err());
    receipt["active"] = json!(true);
    receipt["artifact_sha256"] = json!("z".repeat(64));
    assert!(validate_activation(&receipt, &"a".repeat(40), &"b".repeat(40), "goal").is_err());
}
#[test]
fn host_commands_fail_closed_on_nonzero_or_deadline() {
    let root = std::env::temp_dir();
    assert!(command(&["/bin/sh".into(), "-c".into(), "exit 7".into()], &root, 2).is_err());
    assert!(command(&["/bin/sh".into(), "-c".into(), "sleep 5".into()], &root, 1).is_err());
    assert_eq!(
        command(
            &["/bin/sh".into(), "-c".into(), "printf done".into()],
            &root,
            2
        )
        .unwrap(),
        b"done"
    );
}
