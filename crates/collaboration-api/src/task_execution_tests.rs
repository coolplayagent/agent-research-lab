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

#[test]
fn receipt_validation_detects_old_executors_that_drop_message_metadata() {
    let input = probe_input("claim".into());
    assert!(input.messages.iter().any(|m| m.activity.is_some()));
    assert!(input.messages.iter().any(|m| m.deleted));
    let mut adapter = Configuration::defaults(std::path::Path::new("/fixture"))
        .adapters
        .remove(0);
    adapter.id = "fixture".into();
    adapter.agent = contracts::CodingAgent::Custom;
    let mut result = receipt();
    result.input_sha256 = storage::digest(&serde_json::to_vec(&input).unwrap());
    result.adapter_sha256 = storage::digest(&serde_json::to_vec(&adapter).unwrap());
    result.output_sha256 = storage::digest(&serde_json::to_vec(&result.output).unwrap());
    assert!(validate_receipt(&result, &input, &adapter).is_ok());
    for remove_activity in [true, false] {
        let mut old_input = input.clone();
        for message in &mut old_input.messages {
            if remove_activity {
                message.activity = None;
            } else {
                message.deleted = false;
            }
        }
        result.input_sha256 = storage::digest(&serde_json::to_vec(&old_input).unwrap());
        let error = validate_receipt(&result, &input, &adapter).unwrap_err();
        assert_eq!(error.to_string(), Code::Protocol.as_str());
        assert!(format!("{error:#}").contains("input digest mismatch"));
    }
}

#[test]
fn unavailable_workers_keep_the_specific_reason_in_private_diagnostics() {
    let mut config = Configuration::defaults(std::path::Path::new("/fixture"));
    assert!(
        format!("{:#}", worker_adapter(&config, "worker").unwrap_err())
            .contains("no adapter binding")
    );
    config
        .bindings
        .insert("worker".into(), config.adapters[0].id.clone());
    assert!(
        format!("{:#}", worker_adapter(&config, "worker").unwrap_err())
            .contains("missing or disabled")
    );
    config.adapters[0].enabled = true;
    let error = worker_adapter(&config, "worker").unwrap_err();
    assert_eq!(error.to_string(), Code::Unavailable.as_str());
    assert!(format!("{error:#}").contains("requires binding validation"));
}
