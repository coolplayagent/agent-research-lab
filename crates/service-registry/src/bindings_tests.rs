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
