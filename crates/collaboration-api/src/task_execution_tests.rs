use super::*;

fn receipt() -> ExecutionReceipt {
    ExecutionReceipt {
        output: service_api::AgentOutput {
            protocol_version: 1,
            request_id: "claim".into(),
            summary: "Proposal ready".into(),
            succeeded: true,
            evidence: json!({"coordinator":{"protocol_version":1,"proposal":{"action":"clarify","question":"Which result is needed?"}}}),
        },
        agent: contracts::CodingAgent::Custom,
        adapter_id: "fixture".into(),
        input_sha256: "a".repeat(64),
        adapter_sha256: "b".repeat(64),
        output_sha256: "c".repeat(64),
        sandbox_id: "fixture".into(),
        elapsed_ms: 1,
    }
}

#[test]
fn coordinator_response_rejections_retain_specific_private_reasons() {
    let valid = receipt();
    assert!(coordinator_output(&valid).is_ok());
    let mut failed = valid.clone();
    failed.output.succeeded = false;
    let mut missing = valid.clone();
    missing.output.evidence = json!({});
    let mut unknown = valid.clone();
    unknown.output.evidence["coordinator"]["protocol_version"] = json!(2);
    let mut extra = valid.clone();
    extra.output.evidence["coordinator"]["proposal"]["unexpected"] = json!(true);
    for (bad, reason) in [
        (failed, "succeeded=false"),
        (missing, "Invalid evidence.coordinator"),
        (unknown, "Unsupported coordinator protocol version: 2"),
        (extra, "unknown field"),
    ] {
        let error = coordinator_output(&bad).unwrap_err();
        assert_eq!(error.to_string(), Code::Protocol.as_str());
        assert!(format!("{error:#}").contains(reason));
    }
}
