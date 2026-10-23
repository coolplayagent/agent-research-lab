use super::*;
#[test]
fn downstream_context_includes_host_checks_separately_from_worker_claims() {
    let mut task = OutcomeTask::new(
        TaskInput {
            id: "task".into(),
            group_id: "team".into(),
            intent: "Integrate".into(),
            budget: TaskBudget::default(),
        },
        1000,
    )
    .unwrap();
    let node = |id: &str, dependencies: Vec<String>| TaskNodeInput {
        id: id.into(),
        person_id: "worker".into(),
        title: id.into(),
        instruction: "Integrate evidence".into(),
        deliverable: "Answer".into(),
        acceptance: "Supported answer".into(),
        dependencies,
    };
    task.replace_graph(
        vec![node("first", vec![]), node("next", vec!["first".into()])],
        &["worker".into()].into(),
        1000,
    )
    .unwrap();
    task.nodes[0].state = TaskNodeState::Approved;
    task.nodes[0].artifact_id = Some("artifact-source".into());
    task.nodes[0].review = "The requested result is present".into();
    task.artifacts.push(TaskArtifact {
        id: "artifact-source".into(),
        node_id: "first".into(),
        attempt: 1,
        graph_revision: 1,
        author_id: "worker".into(),
        summary: "Source result".into(),
        evidence: json!({"historical_status":"Awaiting review"}),
        inputs: vec![],
        digest: "a".repeat(64),
        created_ms: 1001,
    });
    let prompt = worker(&task, &task.nodes[1]);
    let checks = prompt
        .split("Host-recorded dependency checks: ")
        .nth(1)
        .unwrap()
        .split('\n')
        .next()
        .unwrap();
    let checks: Value = serde_json::from_str(checks).unwrap();
    assert_eq!(checks[0]["state"], json!(TaskNodeState::Approved));
    assert_eq!(checks[0]["artifact_id"], "artifact-source");
    assert_eq!(checks[0]["review"], task.nodes[0].review);
    assert!(prompt.contains("Awaiting review"));
    assert!(prompt.contains("Source result"));
}

#[test]
fn protocol_repair_uses_only_the_last_finished_attempt_and_preserves_intent() {
    let mut task = OutcomeTask::new(
        TaskInput {
            id: "task".into(),
            group_id: "team".into(),
            intent: "Compare choices".into(),
            budget: TaskBudget::default(),
        },
        1000,
    )
    .unwrap();
    let members = ["worker".into()].into();
    task.transition(
        TaskAction::Claim {
            node_id: None,
            claim_id: "first".into(),
        },
        &members,
        1001,
    )
    .unwrap();
    task.transition(
        TaskAction::Fail {
            node_id: None,
            claim_id: "first".into(),
            code: TaskErrorCode::Protocol,
        },
        &members,
        1002,
    )
    .unwrap();
    task.transition(
        TaskAction::Claim {
            node_id: None,
            claim_id: "retry".into(),
        },
        &members,
        1003,
    )
    .unwrap();
    let prompt = coordinator(&task, &json!([]));
    assert!(prompt.contains("previous coordinator response was rejected"));
    assert!(prompt.contains("Compare choices"));
    assert!(prompt.contains("even while member work or human acceptance is pending"));
    task.transition(
        TaskAction::Coordinate {
            claim_id: "retry".into(),
            output: CoordinatorOutput {
                protocol_version: 1,
                proposal: CoordinatorProposal::Clarify {
                    question: "Which alternatives?".into(),
                },
            },
            receipt: json!({}),
        },
        &members,
        1004,
    )
    .unwrap();
    assert!(!coordinator(&task, &json!([])).contains("previous coordinator response was rejected"));
}
