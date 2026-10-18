use super::*;
fn input() -> AgentInput {
    AgentInput {
        protocol_version: 1,
        request_id: "current-attempt".into(),
        person_id: "planner".into(),
        goal_id: "design".into(),
        work_id: "work-0".into(),
        attempt: 2,
        objective: "Compare plans".into(),
        acceptance: "Provide evidence".into(),
        instruction: "Propose".into(),
        messages: vec![],
    }
}
#[test]
fn native_result_requires_frozen_identity_and_structured_evidence() {
    let input = input();
    let mut result = json!({"protocol_version":1,"request_id":"stale-attempt","summary":"Compared plans","succeeded":true,"evidence_json":"{\"score\":2}"});
    assert!(decode(&input, &serde_json::to_vec(&result).unwrap()).is_err());
    result["request_id"] = json!(input.request_id);
    let output = decode(&input, &serde_json::to_vec(&result).unwrap()).unwrap();
    assert_eq!(output.evidence["score"], 2);
    result["evidence_json"] = json!("null");
    assert!(decode(&input, &serde_json::to_vec(&result).unwrap()).is_err());
    assert_eq!(
        schema(&input)["properties"]["request_id"]["enum"][0],
        input.request_id
    );
}
#[test]
fn credential_config_is_copied_to_a_private_disposable_home_without_memory() {
    use std::os::unix::fs::PermissionsExt;
    let source = tempfile::tempdir().unwrap();
    fs::write(source.path().join("config.toml"), "model = 'example'\n").unwrap();
    fs::write(source.path().join("auth.json"), "{}").unwrap();
    fs::write(
        source.path().join("history.jsonl"),
        "private experiment memory",
    )
    .unwrap();
    let target = source.path().join("isolated");
    copy_configuration(source.path(), &target).unwrap();
    assert!(!target.join("history.jsonl").exists());
    assert_eq!(
        fs::metadata(target.join("auth.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert_eq!(
        fs::read_to_string(target.join("config.toml")).unwrap(),
        "model = 'example'\n"
    );
}
