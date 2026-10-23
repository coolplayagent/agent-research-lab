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
