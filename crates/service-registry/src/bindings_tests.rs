use super::*;
fn request(id: &str) -> BindingRequest {
    BindingRequest {
        request_id: id.into(),
        person_id: "worker".into(),
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
    config.bindings.insert("worker".into(), "codex".into());
    let adapter_digest = storage::digest(
        &serde_json::to_vec(config.adapters.iter().find(|a| a.id == "codex").unwrap()).unwrap(),
    );
    config.binding_evidence.insert(
        "worker".into(),
        BindingEvidence {
            request_id: "first".into(),
            executable_digest: "a".into(),
            adapter_digest,
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
    first.adapter_id = "cc".into();
    operations.begin(first.clone()).unwrap();
    operations.finish(
        "first",
        State::Failed,
        Code::ProgramMissing,
        Some(&anyhow::anyhow!("old diagnostic")),
    );
    let old_log = temp.path().join("private/binding-first.diagnostic.json");
    assert!(old_log.exists());
    let second = request("second");
    operations.begin(second.clone()).unwrap();
    assert!(!old_log.exists());
    assert_eq!(operations.list().len(), 1);
    assert_eq!(operations.list()[0].request.adapter_id, "codex");
    assert!(operations.begin(second).unwrap().1.is_none());
    assert_eq!(
        crate::binding_failure_code(&operations.replay(&first).unwrap_err()),
        Code::Superseded
    );
    assert_eq!(
        crate::binding_failure_code(&operations.get("first").unwrap_err()),
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
        storage::read(&temp.path().join("private/binding-operations.json")).unwrap();
    assert_eq!(journal["operations"].as_array().unwrap().len(), 1);
    assert_eq!(journal["retired"]["first"].as_object().unwrap().len(), 2);
    assert!(!journal.to_string().contains("old diagnostic"));
    drop(operations);
    let recovered = Operations::open(temp.path(), &config).unwrap();
    assert_eq!(recovered.list()[0].state, State::Interrupted);
    assert_eq!(
        crate::binding_failure_code(&recovered.replay(&first).unwrap_err()),
        Code::Superseded
    );
    let mut conflicting = first;
    conflicting.adapter_id = "codex".into();
    assert!(
        recovered
            .replay(&conflicting)
            .unwrap_err()
            .to_string()
            .contains("identity conflict")
    );
}

#[test]
fn legacy_journals_keep_latest_per_person_with_stable_ties() {
    let temp = tempfile::tempdir().unwrap();
    let config = Configuration::defaults(temp.path());
    let operations = Operations::open(temp.path(), &config).unwrap();
    let (mut first, _) = operations.begin(request("first")).unwrap();
    operations.finish("first", State::Failed, Code::ProgramMissing, None);
    let (mut second, _) = operations.begin(request("second")).unwrap();
    let mut another_request = request("another");
    another_request.person_id = "another-person".into();
    let (another, _) = operations.begin(another_request).unwrap();
    first.started_unix_ms = 10;
    second.started_unix_ms = 10;
    let path = temp.path().join("private/binding-operations.json");
    storage::write(&path, &vec![second, another, first.clone()]).unwrap();
    storage::write(
        &temp.path().join("private/binding-first.diagnostic.json"),
        &"obsolete",
    )
    .unwrap();
    drop(operations);
    let recovered = Operations::open(temp.path(), &config).unwrap();
    assert_eq!(recovered.list().len(), 2);
    assert_eq!(recovered.get("second").unwrap().state, State::Interrupted);
    assert!(
        !temp
            .path()
            .join("private/binding-first.diagnostic.json")
            .exists()
    );
    assert_eq!(
        crate::binding_failure_code(&recovered.replay(&first.request).unwrap_err()),
        Code::Superseded
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
    let journal = temp.path().join("private/binding-operations.json");
    std::fs::remove_file(&journal).unwrap();
    std::fs::create_dir(&journal).unwrap();
    assert!(operations.begin(request("second")).is_err());
    assert_eq!(operations.list()[0].request.request_id, "first");
    assert!(operations.replay(&request("first")).unwrap().is_some());
    assert!(
        temp.path()
            .join("private/binding-first.diagnostic.json")
            .exists()
    );
}

#[test]
fn parallel_people_have_independent_latest_attempts_and_bounded_tombstones() {
    let temp = tempfile::tempdir().unwrap();
    let config = Configuration::defaults(temp.path());
    let operations = Operations::open(temp.path(), &config).unwrap();
    for index in 0..4 {
        let mut req = request(&format!("parallel-{index}"));
        req.person_id = format!("person-{index}");
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
    let diagnostic: serde_json::Value =
        storage::read(&temp.path().join("private/binding-first.diagnostic.json")).unwrap();
    assert_eq!(diagnostic["execution"], "fixture execution reason");
    assert!(
        !serde_json::to_string(&operations.list())
            .unwrap()
            .contains("fixture execution reason")
    );
}
