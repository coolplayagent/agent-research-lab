use super::*;
#[tokio::test]
async fn tasks_share_the_storage_owner_and_require_revision() {
    let dir = tempfile::tempdir().unwrap();
    let hub = Hub::open(dir.path()).unwrap();
    hub.register_operator().await.unwrap();
    hub.create_group(NewGroup {
        id: "team".into(),
        title: "Team".into(),
        topic: "Work".into(),
        private: false,
        members: vec!["operator".into()],
        kind: GroupKind::Conversation,
    })
    .await
    .unwrap();
    let input = TaskInput {
        id: "job".into(),
        group_id: "team".into(),
        intent: "Answer".into(),
        budget: TaskBudget::default(),
    };
    let created = hub
        .tasks(TaskCommand::Create {
            input: input.clone(),
        })
        .await
        .unwrap();
    assert_eq!(created["state"], "planning");
    assert_eq!(
        hub.tasks(TaskCommand::Create { input }).await.unwrap()["revision"],
        1
    );
    assert!(
        hub.tasks(TaskCommand::Change {
            id: "job".into(),
            revision: 0,
            action: TaskAction::Cancel
        })
        .await
        .is_err()
    );
    let ended = hub
        .tasks(TaskCommand::Change {
            id: "job".into(),
            revision: 1,
            action: TaskAction::Cancel,
        })
        .await
        .unwrap();
    assert_eq!(ended["state"], "cancelled");
}

async fn apply(hub: &Hub, id: &str, action: TaskAction) -> OutcomeTask {
    let current = hub.tasks(TaskCommand::Get { id: id.into() }).await.unwrap();
    serde_json::from_value(
        hub.tasks(TaskCommand::Change {
            id: id.into(),
            revision: current["revision"].as_u64().unwrap(),
            action,
        })
        .await
        .unwrap(),
    )
    .unwrap()
}
async fn coordinate(hub: &Hub, id: &str, proposal: CoordinatorProposal) -> OutcomeTask {
    apply(
        hub,
        id,
        TaskAction::Claim {
            node_id: None,
            claim_id: "coord".into(),
        },
    )
    .await;
    apply(
        hub,
        id,
        TaskAction::Coordinate {
            claim_id: "coord".into(),
            output: CoordinatorOutput {
                protocol_version: 1,
                proposal,
            },
            receipt: json!({"output_sha256":"a".repeat(64)}),
        },
    )
    .await
}
fn assignment(id: &str, person: &str, deps: Vec<String>) -> TaskNodeInput {
    TaskNodeInput {
        id: id.into(),
        person_id: person.into(),
        title: id.into(),
        instruction: "Produce a supported answer".into(),
        deliverable: "An answer".into(),
        acceptance: "The requested explanation is present".into(),
        dependencies: deps,
    }
}
#[tokio::test]
async fn durable_graph_outcomes_and_human_acceptance_have_distinct_authority() {
    let dir = tempfile::tempdir().unwrap();
    let hub = Hub::open(dir.path()).unwrap();
    hub.register_operator().await.unwrap();
    hub.register(vec!["a".into(), "b".into(), "outsider".into()])
        .await
        .unwrap();
    hub.create_group(NewGroup {
        id: "team".into(),
        title: "Team".into(),
        topic: "Work".into(),
        private: true,
        members: vec!["operator".into(), "a".into(), "b".into()],
        kind: GroupKind::Conversation,
    })
    .await
    .unwrap();
    hub.tasks(TaskCommand::Create {
        input: TaskInput {
            id: "job".into(),
            group_id: "team".into(),
            intent: "Produce an integrated answer".into(),
            budget: TaskBudget::default(),
        },
    })
    .await
    .unwrap();
    coordinate(
        &hub,
        "job",
        CoordinatorProposal::Plan {
            title: "Integrated answer".into(),
            acceptance: "Both sources integrated".into(),
            nodes: vec![
                assignment("first", "a", vec![]),
                assignment("second", "b", vec!["first".into()]),
            ],
        },
    )
    .await;
    let mut previous = None;
    for (node, answer) in [
        ("first", "A unique upstream finding: 817"),
        ("second", "Integration uses the earlier 817 finding"),
    ] {
        apply(
            &hub,
            "job",
            TaskAction::Claim {
                node_id: Some(node.into()),
                claim_id: node.into(),
            },
        )
        .await;
        let submitted = apply(
            &hub,
            "job",
            TaskAction::Submit {
                node_id: node.into(),
                claim_id: node.into(),
                summary: answer.into(),
                evidence: json!({"limitations":"Fixture only"}),
                receipt: json!({"input_sha256":"a".repeat(64)}),
                succeeded: true,
            },
        )
        .await;
        let artifact = submitted.artifacts.last().unwrap();
        if let Some(id) = previous {
            assert_eq!(artifact.inputs, vec![id]);
        }
        previous = Some(artifact.id.clone());
        let outsider = hub.grant("outsider", 60).await.unwrap();
        assert!(
            hub.outcomes(
                Some(&outsider),
                OutcomeCommand::Get {
                    id: artifact.id.clone()
                }
            )
            .await
            .is_err()
        );
        let view = hub
            .outcomes(
                None,
                OutcomeCommand::Get {
                    id: artifact.id.clone(),
                },
            )
            .await
            .unwrap();
        assert_eq!(view["verification"], "unverified");
        assert_eq!(view["acceptance"], "pending");
        coordinate(
            &hub,
            "job",
            CoordinatorProposal::Check {
                reviews: vec![NodeReview {
                    node_id: node.into(),
                    approved: true,
                    reason: "The deliverable is present".into(),
                }],
            },
        )
        .await;
    }
    let delivered = coordinate(
        &hub,
        "job",
        CoordinatorProposal::Deliver {
            summary: "Integrated delivery".into(),
            node_ids: vec!["second".into()],
        },
    )
    .await;
    assert_eq!(delivered.state, contracts::TaskState::Review);
    let accepted = apply(&hub, "job", TaskAction::Accept).await;
    assert_eq!(accepted.state, contracts::TaskState::Completed);
    drop(hub);
    let hub = Hub::open(dir.path()).unwrap();
    let task = hub
        .tasks(TaskCommand::Get { id: "job".into() })
        .await
        .unwrap();
    assert_eq!(task["state"], "completed");
    assert_eq!(task["artifacts"].as_array().unwrap().len(), 2);
    let view = hub
        .outcomes(
            None,
            OutcomeCommand::Get {
                id: previous.unwrap(),
            },
        )
        .await
        .unwrap();
    assert_eq!(view["verification"], "unverified");
    assert_eq!(view["acceptance"], "pending");
}
