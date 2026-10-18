use super::*;
#[test]
fn output_requires_matching_execution_identity_and_protocol() {
    let input = AgentInput {
        protocol_version: 1,
        request_id: "current-claim".into(),
        person_id: "worker".into(),
        goal_id: "goal".into(),
        work_id: "work-1".into(),
        attempt: 2,
        objective: "Compare".into(),
        acceptance: "Evidence required".into(),
        instruction: "Evaluate".into(),
        messages: vec![],
    };
    let mut output = AgentOutput {
        protocol_version: 1,
        request_id: "stale-claim".into(),
        summary: "Completed".into(),
        succeeded: true,
        evidence: serde_json::json!({}),
    };
    assert!(output.validate(&input).is_err());
    output.request_id = input.request_id.clone();
    output.protocol_version = 99;
    assert!(output.validate(&input).is_err());
    output.protocol_version = 1;
    output.summary = "x".repeat(8001);
    assert!(output.validate(&input).is_err());
    output.summary = "Bounded result".into();
    assert!(output.validate(&input).is_ok());
}
