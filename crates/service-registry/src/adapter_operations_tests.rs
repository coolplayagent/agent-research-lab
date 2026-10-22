use super::*;
fn request(id: &str) -> AdapterTestRequest {
    AdapterTestRequest {
        request_id: id.into(),
        adapter_id: "codex".into(),
        revision: 0,
        program: None,
        model: None,
    }
}
#[test]
fn retries_are_idempotent_and_restart_is_interrupted() {
    let temp = tempfile::tempdir().unwrap();
    let config = Configuration::defaults(temp.path());
    let operations = Operations::open(temp.path(), &config).unwrap();
    assert!(operations.begin(request("first")).unwrap().1.is_some());
    assert!(operations.begin(request("first")).unwrap().1.is_none());
    assert!(operations.begin(request("second")).is_err());
    drop(operations);
    let recovered = Operations::open(temp.path(), &config).unwrap();
    assert_eq!(recovered.get("first").unwrap().state, State::Interrupted);
    assert!(recovered.begin(request("second")).unwrap().1.is_some());
}
#[test]
fn committed_evidence_recovers_success_after_journal_failure() {
    let temp = tempfile::tempdir().unwrap();
    let mut config = Configuration::defaults(temp.path());
    let operations = Operations::open(temp.path(), &config).unwrap();
    operations.begin(request("first")).unwrap();
    let adapter_digest = storage::digest(
        &serde_json::to_vec(config.adapters.iter().find(|a| a.id == "codex").unwrap()).unwrap(),
    );
    config.adapter_test_evidence.insert(
        "codex".into(),
        AdapterTestEvidence {
            request_id: "first".into(),
            executable_digest: "a".into(),
            adapter_digest,
            configuration_digest: storage::digest(
                config
                    .adapter_test_configuration("codex")
                    .unwrap()
                    .as_bytes(),
            ),
            input_digest: "c".into(),
            output_digest: "d".into(),
            sandbox_id: "test".into(),
            verified_unix_ms: now(),
        },
    );
    drop(operations);
    assert_eq!(
        Operations::open(temp.path(), &config)
            .unwrap()
            .get("first")
            .unwrap()
            .state,
        State::Succeeded
    );
}

#[test]
fn replacing_an_attempt_deletes_its_logs_and_retains_only_a_replay_fingerprint() {
    let temp = tempfile::tempdir().unwrap();
    let config = Configuration::defaults(temp.path());
    let operations = Operations::open(temp.path(), &config).unwrap();
    let mut first = request("first");
    first.model = Some("first-model".into());
    operations.begin(first.clone()).unwrap();
    operations.finish(
        "first",
        State::Failed,
        Code::ProgramMissing,
        Some(&anyhow::anyhow!("old diagnostic")),
    );
    let old_log = temp
        .path()
        .join("private/adapter-test-first.diagnostic.json");
    assert!(old_log.exists());
    let second = request("second");
    operations.begin(second.clone()).unwrap();
    assert!(!old_log.exists());
    assert_eq!(operations.list().len(), 1);
    assert_eq!(operations.list()[0].request.adapter_id, "codex");
    assert!(operations.begin(second).unwrap().1.is_none());
    assert_eq!(
        crate::adapter_test_failure_code(&operations.replay(&first).unwrap_err()),
        Code::Superseded
    );
    assert_eq!(
        crate::adapter_test_failure_code(&operations.get("first").unwrap_err()),
        Code::Superseded
    );
    operations.finish(
        "first",
        State::Failed,
        Code::VerificationFailed,
        Some(&anyhow::anyhow!("late diagnostic")),
    );
    assert!(
        !old_log.exists(),
        "Late completion cannot recreate a removed log"
    );
    let journal: serde_json::Value =
        storage::read(&temp.path().join("private/adapter-test-operations.json")).unwrap();
    assert_eq!(journal["operations"].as_array().unwrap().len(), 1);
    assert_eq!(journal["retired"]["first"].as_object().unwrap().len(), 2);
    assert!(!journal.to_string().contains("old diagnostic"));
    drop(operations);
    let recovered = Operations::open(temp.path(), &config).unwrap();
    assert_eq!(recovered.list()[0].state, State::Interrupted);
    assert_eq!(
        crate::adapter_test_failure_code(&recovered.replay(&first).unwrap_err()),
        Code::Superseded
    );
    let mut conflicting = first;
    conflicting.model = Some("changed-model".into());
    assert!(
        recovered
            .replay(&conflicting)
            .unwrap_err()
            .to_string()
            .contains("identity conflict")
    );
}

#[test]
fn journal_failure_restores_previous_attempt_and_diagnostic() {
    let temp = tempfile::tempdir().unwrap();
    let config = Configuration::defaults(temp.path());
    let operations = Operations::open(temp.path(), &config).unwrap();
    operations.begin(request("first")).unwrap();
    operations.finish(
        "first",
        State::Failed,
        Code::ProgramMissing,
        Some(&anyhow::anyhow!("fixture")),
    );
    let journal = temp.path().join("private/adapter-test-operations.json");
    std::fs::remove_file(&journal).unwrap();
    std::fs::create_dir(&journal).unwrap();
    assert!(operations.begin(request("second")).is_err());
    assert_eq!(operations.list()[0].request.request_id, "first");
    assert!(operations.replay(&request("first")).unwrap().is_some());
    assert!(
        temp.path()
            .join("private/adapter-test-first.diagnostic.json")
            .exists()
    );
}

#[test]
fn parallel_adapters_have_independent_latest_attempts_and_bounded_tombstones() {
    let temp = tempfile::tempdir().unwrap();
    let config = Configuration::defaults(temp.path());
    let operations = Operations::open(temp.path(), &config).unwrap();
    for index in 0..4 {
        let mut req = request(&format!("parallel-{index}"));
        req.adapter_id = format!("adapter-{index}");
        operations.begin(req).unwrap();
    }
    assert!(operations.begin(request("too-many")).is_err());
    for index in 0..4 {
        operations.finish(
            &format!("parallel-{index}"),
            State::Cancelled,
            Code::Cancelled,
            None,
        );
    }
    for index in 0..270 {
        let id = format!("attempt-{index}");
        operations.begin(request(&id)).unwrap();
        operations.finish(&id, State::Failed, Code::VerificationFailed, None);
    }
    assert_eq!(operations.list().len(), 5);
    assert_eq!(operations.entries.lock().unwrap().retired.len(), 256);
}

#[test]
fn private_execution_details_are_saved_without_entering_the_operation_api() {
    let temp = tempfile::tempdir().unwrap();
    let operations = Operations::open(temp.path(), &Configuration::defaults(temp.path())).unwrap();
    operations.begin(request("first")).unwrap();
    let error = anyhow::Error::new(service_protocol::PrivateDiagnostic(
        "fixture execution reason".into(),
    ));
    operations.finish(
        "first",
        State::Failed,
        Code::VerificationFailed,
        Some(&error),
    );
    let diagnostic: serde_json::Value = storage::read(
        &temp
            .path()
            .join("private/adapter-test-first.diagnostic.json"),
    )
    .unwrap();
    assert_eq!(diagnostic["execution"], "fixture execution reason");
    assert!(
        !serde_json::to_string(&operations.list())
            .unwrap()
            .contains("fixture execution reason")
    );
}
