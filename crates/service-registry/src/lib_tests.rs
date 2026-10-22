use super::*;
#[test]
fn configuration_write_failure_preserves_the_live_revision() {
    let root = tempfile::tempdir().unwrap();
    let registry = Registry::open(root.path()).unwrap();
    let initial = registry.configuration();
    std::fs::remove_file(root.path().join("private/services.json")).unwrap();
    std::fs::create_dir(root.path().join("private/services.json")).unwrap();
    assert!(registry.save(initial.clone()).is_err());
    assert_eq!(
        serde_json::to_value(registry.configuration()).unwrap(),
        serde_json::to_value(initial).unwrap()
    );
}
#[test]
fn settings_cannot_forge_binding_validation() {
    let root = tempfile::tempdir().unwrap();
    let registry = Registry::open(root.path()).unwrap();
    let mut next = registry.configuration();
    next.bindings.insert("worker".into(), "codex".into());
    assert_eq!(
        binding_failure_code(&registry.save_settings(next).unwrap_err()),
        contracts::BindingErrorCode::ValidationRequired
    );
    assert!(registry.configuration().bindings.is_empty());
}
#[tokio::test]
async fn missing_program_is_observable_without_changing_an_existing_binding() {
    let root = tempfile::tempdir().unwrap();
    let registry = std::sync::Arc::new(Registry::open(root.path()).unwrap());
    let mut config = registry.configuration();
    config.bindings.insert("worker".into(), "pi".into());
    let saved = registry.save(config).unwrap();
    let request = BindingRequest {
        request_id: "missing".into(),
        person_id: "worker".into(),
        adapter_id: "codex".into(),
        revision: saved.revision,
        program: Some(root.path().join("absent.exe")),
        model: None,
    };
    registry.start_binding(request.clone()).unwrap();
    let operation = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let operation = registry.binding_operation("missing").unwrap();
            if operation.state != contracts::BindingOperationState::Running {
                break operation;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(operation.state, contracts::BindingOperationState::Failed);
    assert_eq!(
        operation.diagnostic.unwrap().stage,
        contracts::BindingStage::Discover
    );
    assert_eq!(registry.configuration().bindings["worker"], "pi");
    assert_eq!(registry.configuration().revision, saved.revision);
    assert_eq!(
        registry.start_binding(request).unwrap().state,
        contracts::BindingOperationState::Failed
    );
}
#[tokio::test]
async fn cancellation_before_scheduling_never_launches_validation() {
    let root = tempfile::tempdir().unwrap();
    let registry = std::sync::Arc::new(Registry::open(root.path()).unwrap());
    let request = BindingRequest {
        request_id: "cancel-first".into(),
        person_id: "worker".into(),
        adapter_id: "codex".into(),
        revision: 0,
        program: None,
        model: None,
    };
    registry.start_binding(request).unwrap();
    registry.cancel_binding("cancel-first").unwrap();
    tokio::task::yield_now().await;
    assert_eq!(
        registry.binding_operation("cancel-first").unwrap().state,
        contracts::BindingOperationState::Cancelled
    );
    assert!(registry.configuration().bindings.is_empty());
}
#[test]
fn configuration_is_revision_checked_and_dependencies_are_typed() {
    let root = tempfile::tempdir().unwrap();
    let registry = Registry::open(root.path()).unwrap();
    let initial = registry.configuration();
    let mut next = initial.clone();
    next.storage_id = Some("sandbox".into());
    assert!(registry.save(next).is_err());
    let mut next = initial.clone();
    next.adapters[0].enabled = true;
    assert!(registry.save(next).is_err());
    registry.save(initial.clone()).unwrap();
    assert!(registry.save(initial).is_err());
    assert_eq!(
        Registry::open(root.path())
            .unwrap()
            .configuration()
            .revision,
        1
    );
}
#[test]
fn retired_configuration_is_rejected_without_rewriting_private_state() {
    let root = tempfile::tempdir().unwrap();
    let mut config = Configuration::defaults(root.path());
    config.scenarios.push(ScenarioRegistration {
        id: "planning".into(),
        endpoint: root.path().join("private/planning.sock"),
        enabled: false,
        protocol_version: 1,
        capabilities: vec![contracts::ScenarioCapability::Execute],
        dependencies: vec![],
    });
    assert!(
        config
            .validate()
            .unwrap_err()
            .to_string()
            .contains("legacy scenario")
    );
    config.scenarios.clear();
    config
        .outcome_bindings
        .insert("topic".into(), OutcomeServicesBinding::default());
    assert!(
        config
            .validate()
            .unwrap_err()
            .to_string()
            .contains("legacy topic")
    );
    config.outcome_bindings.clear();
    config.outcome_services.push(OutcomeService {
        id: "verifier".into(),
        kind: contracts::OutcomeServiceKind::Verifier,
        management: contracts::ServiceManagement::External,
        endpoint: root.path().join("private/verifier.sock"),
        enabled: false,
        protocol_version: 1,
        outcome_kinds: vec![contracts::OutcomeKind::Finding],
        dependencies: vec![],
        max_seconds: 10,
    });
    assert!(
        config
            .validate()
            .unwrap_err()
            .to_string()
            .contains("legacy verifier")
    );
    let path = root.path().join("private/services.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let original = serde_json::to_vec(&config).unwrap();
    std::fs::write(&path, &original).unwrap();
    assert!(Registry::open(root.path()).is_err());
    assert_eq!(std::fs::read(path).unwrap(), original);
}

#[test]
fn changing_an_existing_service_role_is_rejected_atomically() {
    let root = tempfile::tempdir().unwrap();
    let registry = Registry::open(root.path()).unwrap();
    let initial = registry.configuration();
    let mut next = initial.clone();
    let service = next
        .services
        .iter_mut()
        .find(|s| s.id == "executor")
        .unwrap();
    service.kind = ServiceKind::Sandbox;
    service.sandbox_id = None;
    let error = registry.save(next).unwrap_err();
    assert_eq!(
        error.downcast_ref::<ConfigurationFailure>().unwrap().0,
        contracts::ServiceConfigurationErrorCode::KindImmutable
    );
    assert_eq!(
        serde_json::to_value(registry.configuration()).unwrap(),
        serde_json::to_value(initial).unwrap()
    );
    assert!(root.path().join("private/services.json").is_file());
}
#[test]
fn builtin_adapter_templates_save_but_cannot_be_enabled_by_settings() {
    let root = tempfile::tempdir().unwrap();
    let registry = Registry::open(root.path()).unwrap();
    let saved = registry.save_settings(registry.configuration()).unwrap();
    let mut next = saved.clone();
    let adapter = next.adapters.iter_mut().find(|a| a.id == "codex").unwrap();
    adapter.enabled = true;
    adapter.command = vec![
        root.path()
            .join("fake-command")
            .to_string_lossy()
            .into_owned(),
    ];
    let error = registry.save_settings(next).unwrap_err();
    assert_eq!(
        error.downcast_ref::<ConfigurationFailure>().unwrap().0,
        contracts::ServiceConfigurationErrorCode::ValidationRequired
    );
    assert_eq!(registry.configuration().revision, saved.revision);
}

#[test]
fn settings_cannot_reassign_managed_ownership_or_create_managed_endpoints() {
    let root = tempfile::tempdir().unwrap();
    let registry = Registry::open(root.path()).unwrap();
    let before = registry.configuration();
    let mut next = before.clone();
    next.services[0].management = contracts::ServiceManagement::External;
    assert_eq!(
        registry
            .save_settings(next)
            .unwrap_err()
            .downcast_ref::<ConfigurationFailure>()
            .unwrap()
            .0,
        contracts::ServiceConfigurationErrorCode::ManagementImmutable
    );
    let mut next = before.clone();
    let mut forged = next.services[0].clone();
    forged.id = "forged".into();
    next.services.push(forged);
    assert!(registry.save_settings(next).is_err());
    assert_eq!(
        serde_json::to_value(registry.configuration()).unwrap(),
        serde_json::to_value(before).unwrap()
    );
}
