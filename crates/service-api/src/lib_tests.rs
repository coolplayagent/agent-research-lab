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

#[test]
fn proxy_environment_names_are_explicit_without_allowing_runtime_injection() {
    let mut adapter = Configuration::defaults(std::path::Path::new("/fixture"))
        .adapters
        .remove(0);
    adapter.env_names = NETWORK_ENVIRONMENT_NAMES
        .iter()
        .map(|s| (*s).into())
        .collect();
    assert!(adapter.validate().is_ok());
    for name in [
        "LD_PRELOAD",
        "LD_LIBRARY_PATH",
        "NODE_OPTIONS",
        "BASH_ENV",
        "ENV",
        "PYTHONPATH",
        "arbitrary_lowercase",
        "https_proxy=secret",
        "HTTP_PROXY;command",
    ] {
        adapter.env_names = vec![name.into()];
        assert!(adapter.validate().is_err(), "{name}");
    }
}
