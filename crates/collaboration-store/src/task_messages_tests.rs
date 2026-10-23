use super::*;
#[tokio::test]
async fn ordinary_messages_are_durable_and_idempotent_but_announcements_are_not_tasks() {
    let dir = tempfile::tempdir().unwrap();
    let hub = Hub::open(dir.path()).unwrap();
    hub.register_operator().await.unwrap();
    for (id, kind) in [
        ("team", GroupKind::Conversation),
        ("news", GroupKind::Board),
    ] {
        hub.create_group(NewGroup {
            id: id.into(),
            title: id.into(),
            topic: "Collaboration".into(),
            private: false,
            members: vec!["operator".into()],
            kind,
        })
        .await
        .unwrap();
    }
    let message = Publish {
        group_id: "team".into(),
        request_id: "hello".into(),
        text: "Please draft a plan".into(),
        reply_to: None,
    };
    let first = hub
        .tasks(TaskCommand::Message {
            message: message.clone(),
        })
        .await
        .unwrap();
    let second = hub
        .tasks(TaskCommand::Message {
            message: message.clone(),
        })
        .await
        .unwrap();
    assert_eq!(first["task"]["input"]["id"], second["task"]["input"]["id"]);
    assert_eq!(second["duplicate"], true);
    assert_eq!(first["task"]["kind"], "message");
    assert_eq!(hub.history("team", 0, 100).await.unwrap().len(), 2);
    let mut announcement = message;
    announcement.group_id = "news".into();
    let news = hub
        .tasks(TaskCommand::Message {
            message: announcement,
        })
        .await
        .unwrap();
    assert!(news.get("task").is_none());
    drop(hub);
    let hub = Hub::open(dir.path()).unwrap();
    assert_eq!(
        hub.tasks(TaskCommand::Get {
            id: first["task"]["input"]["id"].as_str().unwrap().into()
        })
        .await
        .unwrap()["state"],
        "planning"
    );
    assert_eq!(
        hub.history("team", 0, 100).await.unwrap()[1]
            .activity
            .as_ref()
            .unwrap()
            .kind,
        contracts::TaskEventKind::Created
    );
}

async fn edit(hub: &Hub, id: &str, action: TaskAction) -> Result<OutcomeTask> {
    let current = hub.tasks(TaskCommand::Get { id: id.into() }).await?;
    Ok(serde_json::from_value(
        hub.tasks(TaskCommand::Change {
            id: id.into(),
            revision: current["revision"].as_u64().unwrap(),
            action,
        })
        .await?,
    )?)
}
async fn propose(hub: &Hub, id: &str, proposal: CoordinatorProposal) -> Result<OutcomeTask> {
    edit(
        hub,
        id,
        TaskAction::Claim {
            node_id: None,
            claim_id: "routing-claim".into(),
        },
    )
    .await?;
    edit(
        hub,
        id,
        TaskAction::Coordinate {
            claim_id: "routing-claim".into(),
            output: CoordinatorOutput {
                protocol_version: 1,
                proposal,
            },
            receipt: json!({}),
        },
    )
    .await
}
async fn room(hub: &Hub, id: &str) {
    hub.create_group(NewGroup {
        id: id.into(),
        title: id.into(),
        topic: "Collaboration".into(),
        private: false,
        members: vec!["operator".into(), "worker".into()],
        kind: GroupKind::Conversation,
    })
    .await
    .unwrap();
}
async fn send(hub: &Hub, request: &str, reply: Option<u64>) -> OutcomeTask {
    serde_json::from_value(
        hub.tasks(TaskCommand::Message {
            message: Publish {
                group_id: "team".into(),
                request_id: request.into(),
                text: format!("User request {request}"),
                reply_to: reply,
            },
        })
        .await
        .unwrap()["task"]
            .clone(),
    )
    .unwrap()
}
fn node() -> TaskNodeInput {
    TaskNodeInput {
        id: "n1".into(),
        person_id: "worker".into(),
        title: "Draft".into(),
        instruction: "Draft an answer".into(),
        deliverable: "Answer".into(),
        acceptance: "A supported answer".into(),
        dependencies: vec![],
    }
}
#[tokio::test]
async fn intake_supports_clarification_supplement_and_response_without_false_acceptance() {
    let dir = tempfile::tempdir().unwrap();
    let hub = Hub::open(dir.path()).unwrap();
    hub.register_operator().await.unwrap();
    hub.register(vec!["worker".into()]).await.unwrap();
    room(&hub, "team").await;
    let a = send(&hub, "first", None).await;
    let waiting = propose(
        &hub,
        &a.input.id,
        CoordinatorProposal::Clarify {
            question: "Which audience?".into(),
        },
    )
    .await
    .unwrap();
    let reply = send(&hub, "audience", waiting.message_sequences.last().copied()).await;
    assert_eq!(reply.reply_task_id.as_deref(), Some(a.input.id.as_str()));
    // Ticking does not invalidate the coordinator's snapshot of the user's intent.
    edit(&hub, &a.input.id, TaskAction::Tick).await.unwrap();
    let routed = propose(
        &hub,
        &reply.input.id,
        CoordinatorProposal::Attach {
            task_id: a.input.id.clone(),
            revision: waiting.revision,
            text: "The audience is beginners".into(),
        },
    )
    .await
    .unwrap();
    assert_eq!(routed.state, contracts::TaskState::Completed);
    assert!(routed.deliveries.is_empty());
    let target = hub
        .tasks(TaskCommand::Get {
            id: a.input.id.clone(),
        })
        .await
        .unwrap();
    assert_eq!(target["state"], "planning");
    assert_eq!(target["notes"][0], "The audience is beginners");
    let planned = propose(
        &hub,
        &a.input.id,
        CoordinatorProposal::Plan {
            title: "A clear guide".into(),
            acceptance: "Readable by beginners".into(),
            nodes: vec![node()],
        },
    )
    .await
    .unwrap();
    assert_eq!(planned.kind, contracts::TaskKind::Outcome);
    assert!(hub.history("team", 0, 100).await.unwrap().iter().any(|m| {
        m.activity
            .as_ref()
            .is_some_and(|a| a.kind == contracts::TaskEventKind::Planned)
    }));
    let greeting = send(&hub, "hello", None).await;
    let response = propose(
        &hub,
        &greeting.input.id,
        CoordinatorProposal::Respond {
            message: "Hello. What would you like to achieve?".into(),
        },
    )
    .await
    .unwrap();
    assert_eq!(response.state, contracts::TaskState::Completed);
    assert!(response.deliveries.is_empty());
    assert!(response.nodes.is_empty());
    let page = hub
        .tasks(TaskCommand::List {
            group_id: "team".into(),
            after: String::new(),
            active: false,
            questions: false,
        })
        .await
        .unwrap();
    assert_eq!(page["tasks"].as_array().unwrap().len(), 1);
    // The host's chat projections never feed themselves back into intake.
    let all = hub
        .tasks(TaskCommand::List {
            group_id: "team".into(),
            after: String::new(),
            active: false,
            questions: true,
        })
        .await
        .unwrap();
    assert_eq!(all["tasks"].as_array().unwrap().len(), 3);
}
#[tokio::test]
async fn routing_cannot_cross_groups_or_overwrite_a_concurrent_human_edit() {
    let dir = tempfile::tempdir().unwrap();
    let hub = Hub::open(dir.path()).unwrap();
    hub.register_operator().await.unwrap();
    hub.register(vec!["worker".into()]).await.unwrap();
    room(&hub, "team").await;
    room(&hub, "other").await;
    hub.tasks(TaskCommand::Create {
        input: TaskInput {
            id: "outside".into(),
            group_id: "other".into(),
            intent: "Other room".into(),
            budget: TaskBudget::default(),
        },
    })
    .await
    .unwrap();
    let source = send(&hub, "source", None).await;
    assert!(
        propose(
            &hub,
            &source.input.id,
            CoordinatorProposal::Attach {
                task_id: "outside".into(),
                revision: 1,
                text: "Bad association".into()
            }
        )
        .await
        .is_err()
    );
    let target = send(&hub, "target", None).await;
    edit(
        &hub,
        &target.input.id,
        TaskAction::Supplement {
            text: "Explicit new constraint".into(),
        },
    )
    .await
    .unwrap();
    assert!(
        edit(
            &hub,
            &source.input.id,
            TaskAction::Coordinate {
                claim_id: "routing-claim".into(),
                output: CoordinatorOutput {
                    protocol_version: 1,
                    proposal: CoordinatorProposal::Attach {
                        task_id: target.input.id.clone(),
                        revision: target.revision,
                        text: "Old intent".into()
                    }
                },
                receipt: json!({})
            }
        )
        .await
        .is_err()
    );
    assert_eq!(
        hub.tasks(TaskCommand::Get {
            id: source.input.id
        })
        .await
        .unwrap()["state"],
        "planning"
    );
    assert_eq!(
        hub.tasks(TaskCommand::Get {
            id: target.input.id
        })
        .await
        .unwrap()["notes"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}
#[tokio::test]
async fn presence_follows_concurrent_leases_and_survives_restart_without_connections() {
    let dir = tempfile::tempdir().unwrap();
    let mut hub = Hub::open(dir.path()).unwrap();
    hub.register_operator().await.unwrap();
    hub.register(vec!["worker".into()]).await.unwrap();
    room(&hub, "team").await;
    assert_eq!(hub.presence("worker").unwrap().presence, Presence::Online);
    let mut ids = vec![];
    for request in ["one", "two"] {
        let task = send(&hub, request, None).await;
        propose(
            &hub,
            &task.input.id,
            CoordinatorProposal::Plan {
                title: "Work".into(),
                acceptance: "Answer".into(),
                nodes: vec![node()],
            },
        )
        .await
        .unwrap();
        edit(
            &hub,
            &task.input.id,
            TaskAction::Claim {
                node_id: Some("n1".into()),
                claim_id: request.into(),
            },
        )
        .await
        .unwrap();
        ids.push(task.input.id);
    }
    assert_eq!(hub.presence("worker").unwrap().presence, Presence::Busy);
    assert_eq!(hub.presence("worker").unwrap().connections, 0);
    drop(hub);
    hub = Hub::open(dir.path()).unwrap();
    assert_eq!(hub.presence("worker").unwrap().presence, Presence::Busy);
    edit(&hub, &ids[0], TaskAction::Cancel).await.unwrap();
    assert_eq!(hub.presence("worker").unwrap().presence, Presence::Busy);
    edit(
        &hub,
        &ids[1],
        TaskAction::Fail {
            node_id: Some("n1".into()),
            claim_id: "two".into(),
            code: contracts::TaskErrorCode::Unavailable,
        },
    )
    .await
    .unwrap();
    assert_eq!(hub.presence("worker").unwrap().presence, Presence::Online);
}

#[tokio::test]
async fn archived_historical_members_are_explicit_without_changing_active_wire_shape() {
    let dir = tempfile::tempdir().unwrap();
    let hub = Hub::open(dir.path()).unwrap();
    hub.register_operator().await.unwrap();
    hub.register(vec!["worker".into()]).await.unwrap();
    room(&hub, "team").await;
    let active = serde_json::to_value(hub.presence("worker").unwrap()).unwrap();
    assert!(active.get("archived").is_none());
    assert_eq!(active["presence"], "online");
    hub.archive_unbound(BTreeSet::new()).await.unwrap();
    let historical = hub.members("team", "", 100).unwrap();
    let worker = historical["members"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["person_id"] == "worker")
        .unwrap();
    assert_eq!(worker["archived"], true);
    assert_eq!(worker["presence"], "offline");
    hub.save_person(Person {
        id: "worker".into(),
        name: "Worker".into(),
        application_id: None,
    })
    .await
    .unwrap();
    let restored = hub.presence("worker").unwrap();
    assert!(!restored.archived);
    assert_eq!(restored.presence, Presence::Online);
}
