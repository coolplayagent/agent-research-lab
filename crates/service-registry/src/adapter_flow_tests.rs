use super::*;
fn request(registry: &Registry, id: &str) -> AdapterTestRequest {
    AdapterTestRequest {
        request_id: id.into(),
        adapter_id: "codex".into(),
        revision: registry.configuration().revision,
        program: None,
        model: None,
    }
}
#[tokio::test]
async fn cancelled_adapter_test_does_not_create_binding_or_enable_template() {
    let root = tempfile::tempdir().unwrap();
    let registry = Arc::new(Registry::open(root.path()).unwrap());
    let before = serde_json::to_value(registry.configuration()).unwrap();
    registry
        .start_adapter_test(request(&registry, "cancel-first"))
        .unwrap();
    registry.cancel_adapter_test("cancel-first").unwrap();
    tokio::task::yield_now().await;
    let result = registry.adapter_test_operation("cancel-first").unwrap();
    assert_eq!(result.state, State::Cancelled);
    assert!(result.steps.iter().all(|s| s.state == Step::Skipped));
    assert_eq!(
        serde_json::to_value(registry.configuration()).unwrap(),
        before
    );
    assert!(registry.binding_operations().is_empty());
}
#[tokio::test]
async fn discovery_failure_keeps_configuration_and_replays_without_another_attempt() {
    let root = tempfile::tempdir().unwrap();
    let registry = Arc::new(Registry::open(root.path()).unwrap());
    let before = serde_json::to_value(registry.configuration()).unwrap();
    let mut req = request(&registry, "missing");
    req.program = Some(root.path().join("missing-agent"));
    registry.start_adapter_test(req.clone()).unwrap();
    let operation = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let op = registry.adapter_test_operation("missing").unwrap();
            if op.state != State::Running {
                break op;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(operation.state, State::Failed);
    assert_eq!(operation.diagnostic.unwrap().code, Code::ProgramMissing);
    assert_eq!(
        serde_json::to_value(registry.configuration()).unwrap(),
        before
    );
    assert_eq!(
        registry.start_adapter_test(req).unwrap().state,
        State::Failed
    );
    assert_eq!(registry.adapter_test_operations().len(), 1);
    assert!(registry.binding_operations().is_empty());
}
#[tokio::test]
async fn adapter_and_binding_operations_share_bounded_capacity() {
    let root = tempfile::tempdir().unwrap();
    let registry = Arc::new(Registry::open(root.path()).unwrap());
    let permits = registry
        .validation_slots
        .clone()
        .acquire_many_owned(4)
        .await
        .unwrap();
    assert_eq!(
        adapter_test_failure_code(
            &registry
                .start_adapter_test(request(&registry, "full"))
                .unwrap_err()
        ),
        Code::Busy
    );
    let binding = BindingRequest {
        request_id: "binding-full".into(),
        person_id: "worker".into(),
        adapter_id: "codex".into(),
        revision: 0,
        program: None,
        model: None,
    };
    assert_eq!(
        crate::binding_failure_code(&registry.start_binding(binding).unwrap_err()),
        contracts::BindingErrorCode::Busy
    );
    assert!(registry.adapter_test_operations().is_empty());
    drop(permits);
    let mut stale = request(&registry, "stale");
    stale.revision += 1;
    assert_eq!(
        adapter_test_failure_code(&registry.start_adapter_test(stale).unwrap_err()),
        Code::RevisionConflict
    );
}
#[test]
fn settings_cannot_forge_test_evidence_and_dependency_changes_invalidate_it() {
    let root = tempfile::tempdir().unwrap();
    let registry = Registry::open(root.path()).unwrap();
    let mut config = registry.configuration();
    let evidence = AdapterTestEvidence {
        request_id: "host-test".into(),
        executable_digest: "fixture".into(),
        adapter_digest: storage::digest(
            &serde_json::to_vec(config.adapters.iter().find(|a| a.id == "codex").unwrap()).unwrap(),
        ),
        configuration_digest: storage::digest(
            config
                .adapter_test_configuration("codex")
                .unwrap()
                .as_bytes(),
        ),
        input_digest: "input".into(),
        output_digest: "output".into(),
        sandbox_id: "fixture".into(),
        verified_unix_ms: 1,
    };
    config
        .adapter_test_evidence
        .insert("codex".into(), evidence.clone());
    let saved = registry.save_settings(config).unwrap();
    assert!(saved.adapter_test_evidence.is_empty());
    let mut host = saved;
    host.adapter_test_evidence.insert("codex".into(), evidence);
    let saved = registry.save(host).unwrap();
    assert!(saved.adapter_test_evidence.contains_key("codex"));
    let mut changed = saved;
    changed
        .services
        .iter_mut()
        .find(|s| s.id == "sandbox")
        .unwrap()
        .enabled = false;
    changed
        .services
        .iter_mut()
        .find(|s| s.id == "executor")
        .unwrap()
        .enabled = false;
    let saved = registry.save_settings(changed).unwrap();
    assert!(saved.adapter_test_evidence.is_empty());
    assert!(saved.bindings.is_empty());
}
