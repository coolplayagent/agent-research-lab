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
