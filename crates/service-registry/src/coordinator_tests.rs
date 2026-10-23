use super::*;
use serde_json::json;

#[test]
fn rejected_task_receipts_are_private_bounded_and_bound_to_the_claim() {
    let root = tempfile::tempdir().unwrap();
    let registry = Registry::open(root.path()).unwrap();
    let configuration = serde_json::to_value(registry.configuration()).unwrap();
    let mut receipt = ExecutionReceipt {
        output: AgentOutput {
            protocol_version: 1,
            request_id: "claim".into(),
            summary: "Private failure explanation".into(),
            succeeded: false,
            evidence: json!({"coordinator":{"invalid":"fixture"}}),
        },
        agent: contracts::CodingAgent::Custom,
        adapter_id: "fixture".into(),
        input_sha256: "a".repeat(64),
        adapter_sha256: "b".repeat(64),
        output_sha256: "c".repeat(64),
        sandbox_id: "fixture".into(),
        elapsed_ms: 1,
    };
    let error = anyhow::anyhow!("Coordinator reported succeeded=false")
        .context(contracts::TaskErrorCode::Protocol);
    registry.record_task_diagnostic("task", "claim", &error, Some(&receipt));
    let path = root.path().join(format!(
        "private/task-{}.diagnostic.json",
        &storage::digest(b"task")[..32]
    ));
    let diagnostic: serde_json::Value = storage::read(&path).unwrap();
    assert_eq!(diagnostic["claim_id"], "claim");
    assert_eq!(diagnostic["task_id"], "task");
    assert_eq!(diagnostic["receipt"], json!(receipt));
    assert!(
        diagnostic["details"]
            .as_str()
            .unwrap()
            .contains("succeeded=false")
    );
    assert_eq!(
        serde_json::to_value(registry.configuration()).unwrap(),
        configuration
    );
    receipt.output.summary = "x".repeat(65537);
    registry.record_task_diagnostic("task", "next-claim", &error, Some(&receipt));
    let diagnostic: serde_json::Value = storage::read(&path).unwrap();
    assert_eq!(diagnostic["claim_id"], "next-claim");
    assert!(diagnostic["receipt"].is_null());
}

#[test]
fn previous_empty_message_probe_proof_cannot_skip_current_contract_validation() {
    let mut config = Configuration::defaults(std::path::Path::new("/fixture"));
    let adapter = &mut config.adapters[0];
    adapter.enabled = true;
    adapter.command = vec!["fixture".into()];
    config.coordinator = Some(CoordinatorConfiguration {
        adapter_id: adapter.id.clone(),
        model: None,
        adapter_digest: storage::digest(&serde_json::to_vec(&adapter).unwrap()),
        protocol_evidence: None,
    });
    let adapter = Registry::coordinator_adapter(&config).unwrap();
    let executor = config
        .service(&adapter.executor_id, contracts::ServiceKind::Executor)
        .unwrap();
    let sandbox = config
        .service(
            executor.sandbox_id.as_deref().unwrap(),
            contracts::ServiceKind::Sandbox,
        )
        .unwrap();
    let old = storage::digest(&serde_json::to_vec(&(adapter, executor, sandbox)).unwrap());
    config.coordinator.as_mut().unwrap().protocol_evidence =
        Some(format!("{old}:{}", "a".repeat(64)));
    assert!(!Registry::coordinator_probe_valid(&config));
    let current = Registry::coordinator_fingerprint(&config).unwrap();
    config.coordinator.as_mut().unwrap().protocol_evidence =
        Some(format!("{current}:{}", "a".repeat(64)));
    assert!(Registry::coordinator_probe_valid(&config));
}

fn configured_registry(root: &Path) -> Registry {
    let registry = Registry::open(root).unwrap();
    let mut config = registry.configuration();
    let adapter = &mut config.adapters[0];
    adapter.enabled = true;
    adapter.command = vec![
        root.join("outcome-hub-agent")
            .to_string_lossy()
            .into_owned(),
    ];
    let digest = storage::digest(&serde_json::to_vec(adapter).unwrap());
    config.coordinator = Some(CoordinatorConfiguration {
        adapter_id: adapter.id.clone(),
        model: Some("selected-model".into()),
        adapter_digest: digest.clone(),
        protocol_evidence: None,
    });
    config.bindings.insert("worker".into(), adapter.id.clone());
    config.binding_evidence.insert(
        "worker".into(),
        BindingEvidence {
            request_id: "verified-worker".into(),
            executable_digest: "a".repeat(64),
            adapter_digest: digest,
            input_digest: "b".repeat(64),
            output_digest: "c".repeat(64),
            sandbox_id: "fixture".into(),
            verified_unix_ms: 1,
        },
    );
    let saved = registry.save(config).unwrap();
    registry
        .record_coordinator_probe(&saved, "d".repeat(64))
        .unwrap();
    registry
}

#[test]
fn adapter_changes_refresh_selection_without_reusing_validation_evidence() {
    let root = tempfile::tempdir().unwrap();
    let registry = configured_registry(root.path());
    let before = registry.configuration();
    assert!(Registry::coordinator_probe_valid(&before));
    let mut next = before.clone();
    next.adapters[0].max_seconds += 1;
    let saved = registry.save(next).unwrap();
    let selected = saved.coordinator.as_ref().unwrap();
    assert_eq!(
        selected.adapter_id,
        before.coordinator.as_ref().unwrap().adapter_id
    );
    assert_eq!(selected.model.as_deref(), Some("selected-model"));
    assert!(selected.protocol_evidence.is_none());
    assert!(!Registry::coordinator_probe_valid(&saved));
    assert!(Registry::coordinator_adapter(&saved).is_ok());
    assert!(
        saved.binding_evidence.is_empty(),
        "Changed worker configurations need real revalidation"
    );
    assert_eq!(saved.bindings, before.bindings);
    assert!(
        registry
            .record_coordinator_probe(&before, "e".repeat(64))
            .is_err(),
        "A late old probe cannot validate new configuration"
    );
    registry
        .record_coordinator_probe(&saved, "f".repeat(64))
        .unwrap();
    let fresh = registry.configuration();
    assert!(Registry::coordinator_probe_valid(&fresh));
    let unchanged = registry.save(fresh.clone()).unwrap();
    assert_eq!(
        unchanged.coordinator.unwrap().protocol_evidence,
        fresh.coordinator.unwrap().protocol_evidence
    );
    assert!(Registry::coordinator_probe_valid(
        &Registry::open(root.path()).unwrap().configuration()
    ));
}

#[test]
fn persisted_stale_selection_is_repaired_but_disabled_adapters_remain_unavailable() {
    let root = tempfile::tempdir().unwrap();
    let registry = configured_registry(root.path());
    let mut legacy = registry.configuration();
    legacy.adapters[0].max_seconds += 1;
    storage::write(&root.path().join("private/services.json"), &legacy).unwrap();
    let registry = Registry::open(root.path()).unwrap();
    let repaired = registry.coordinator_configuration().unwrap();
    assert_eq!(repaired.revision, legacy.revision + 1);
    assert!(
        repaired
            .coordinator
            .as_ref()
            .unwrap()
            .protocol_evidence
            .is_none()
    );
    assert!(!Registry::coordinator_probe_valid(&repaired));
    assert!(repaired.binding_evidence.is_empty());
    assert_eq!(
        registry.coordinator_configuration().unwrap().revision,
        repaired.revision
    );
    registry
        .record_coordinator_probe(&repaired, "e".repeat(64))
        .unwrap();
    let mut disabled = registry.configuration();
    disabled.adapters[0].enabled = false;
    let saved = registry.save(disabled).unwrap();
    assert!(
        saved
            .coordinator
            .as_ref()
            .unwrap()
            .protocol_evidence
            .is_none()
    );
    let error = registry.coordinator_configuration().unwrap_err();
    assert!(format!("{error:#}").contains("missing or disabled"));
    assert!(!Registry::coordinator_probe_valid(&saved));
}

#[test]
fn execution_service_changes_invalidate_the_coordinator_probe() {
    let root = tempfile::tempdir().unwrap();
    let registry = configured_registry(root.path());
    let mut changed = registry.configuration();
    let executor = changed
        .services
        .iter_mut()
        .find(|s| s.kind == ServiceKind::Executor)
        .unwrap();
    executor.management = contracts::ServiceManagement::External;
    executor.endpoint = root
        .path()
        .join("private/services/replacement-executor.sock");
    let saved = registry.save(changed).unwrap();
    assert!(saved.coordinator.unwrap().protocol_evidence.is_none());
}
