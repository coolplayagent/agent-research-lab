use super::*;
#[tokio::test]
async fn executor_rejects_disabled_adapter_before_launch() {
    let adapter = service_api::Adapter {
        id: "hds".into(),
        agent: contracts::CodingAgent::DeepseekHarness,
        protocol: contracts::AgentProtocol::JsonStdioV1,
        enabled: false,
        executor_id: "executor".into(),
        command: vec![],
        env_names: vec![],
        read_only_paths: vec![],
        network: false,
        max_seconds: 30,
    };
    let input = service_api::AgentInput {
        protocol_version: 1,
        request_id: "request".into(),
        person_id: "worker".into(),
        goal_id: "goal".into(),
        work_id: "work-1".into(),
        attempt: 0,
        objective: "objective".into(),
        acceptance: "acceptance".into(),
        instruction: "instruction".into(),
        messages: vec![],
    };
    assert!(
        execute(ExecutorRequest::Execute {
            adapter,
            sandbox: "/tmp/absent.sock".into(),
            input: Box::new(input),
            seconds: 30
        })
        .await
        .unwrap_err()
        .to_string()
        .contains("disabled")
    );
}
