use super::*;
#[tokio::test]
async fn remote_iteration_commands_preserve_revision_fences_and_change_events() {
    let root = tempfile::tempdir().unwrap();
    let socket = root.path().join("private/iteration.sock");
    let state = root.path().join("state");
    let service = tokio::spawn({
        let socket = socket.clone();
        async move { serve(&state, &socket, 60).await }
    });
    let client = Client::remote(socket.clone()).unwrap();
    for _ in 0..100 {
        if socket.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    client.register_operator().await.unwrap();
    client.register(vec!["worker".into()]).await.unwrap();
    client
        .create_group(NewGroup {
            id: "room".into(),
            title: "Plans".into(),
            topic: "Iterate plans".into(),
            kind: GroupKind::Conversation,
            private: false,
            members: vec!["operator".into(), "worker".into()],
        })
        .await
        .unwrap();
    client
        .research_change(ResearchChange::Topic {
            topic: CollaborationTopic {
                id: "topic".into(),
                group_id: "room".into(),
                title: "Plans".into(),
                objective: "Compare plans".into(),
                state: contracts::TopicState::Active,
                revision: 0,
            },
        })
        .await
        .unwrap();
    client
        .research_change(ResearchChange::Subject {
            subject: CollaborationSubject {
                id: "subject".into(),
                topic_id: "topic".into(),
                name: "Plan".into(),
                kind: contracts::SubjectKind::Other,
                reference: "plan/example".into(),
                revision: 0,
            },
        })
        .await
        .unwrap();
    client
        .research_change(ResearchChange::Node {
            node: EvolutionInput {
                id: "baseline".into(),
                subject_id: "subject".into(),
                parents: vec![],
                version: "1".into(),
                reference: "plan/example/1".into(),
                goal_id: None,
                pins: EvidencePins::default(),
            },
        })
        .await
        .unwrap();
    let input = EvolutionRunInput {
        id: "iteration".into(),
        group_id: "room".into(),
        subject_id: "subject".into(),
        scenario_id: "plan-comparison".into(),
        title: "Compare".into(),
        objective: "Reduce cost".into(),
        acceptance: "Record measured cost".into(),
        assignments: vec![AssignmentInput {
            person_id: "worker".into(),
            instruction: "Measure plan cost".into(),
        }],
        baseline_version_id: "baseline".into(),
        parameters: json!({}),
        max_rounds: 1,
        max_seconds: 60,
        total_budget_seconds: 60,
    };
    let mut events = client.changes().await.unwrap();
    let create = EvolutionChange {
        request_id: "create".into(),
        run_id: "iteration".into(),
        revision: 0,
        action: EvolutionAction::Create { input },
    };
    let run = client.evolution_change(create.clone()).await.unwrap();
    tokio::time::timeout(Duration::from_secs(2), events.changed())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        client.evolution_change(create).await.unwrap().revision,
        run.revision
    );
    let run = client
        .evolution_change(EvolutionChange {
            request_id: "start".into(),
            run_id: "iteration".into(),
            revision: run.revision,
            action: EvolutionAction::StartRound {
                goal: run.next_goal().unwrap(),
            },
        })
        .await
        .unwrap();
    let paused = client
        .evolution_change(EvolutionChange {
            request_id: "pause".into(),
            run_id: "iteration".into(),
            revision: run.revision,
            action: EvolutionAction::Pause,
        })
        .await
        .unwrap();
    assert_eq!(paused.state, contracts::IterationState::Paused);
    assert!(
        client
            .evolution_change(EvolutionChange {
                request_id: "stale".into(),
                run_id: "iteration".into(),
                revision: run.revision,
                action: EvolutionAction::Stop
            })
            .await
            .is_err()
    );
    let stop = client
        .evolution_change(EvolutionChange {
            request_id: "stop".into(),
            run_id: "iteration".into(),
            revision: paused.revision,
            action: EvolutionAction::Stop,
        })
        .await
        .unwrap();
    assert_eq!(
        client.evolution_run("iteration").await.unwrap().revision,
        stop.revision
    );
    assert_eq!(
        client
            .evolution_runs("room", "plan-comparison", "", false)
            .await
            .unwrap()["runs"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(
        client.evolution_runs("room", "", "", true).await.unwrap()["runs"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        client.goals("room", "", "", false).await.unwrap()["goals"][0]["state"],
        contracts::GoalState::Cancelled.as_str()
    );
    let current = client.current_goal("room").await.unwrap();
    assert_eq!(current["goals"].as_array().unwrap().len(), 1);
    assert_eq!(current["goals"][0]["iteration_id"], "iteration");
    assert_eq!(
        current["goals"][0]["state"],
        contracts::GoalState::Cancelled.as_str()
    );
    assert!(current["next_after"].is_null());
    assert!(client.current_goal("").await.is_err());
    service.abort();
    let _ = service.await;
}

#[tokio::test]
async fn remote_storage_preserves_durability_authority_and_stream_controls() {
    let root = tempfile::tempdir().unwrap();
    let socket = root.path().join("private/store.sock");
    let state = root.path().join("db");
    let service = tokio::spawn({
        let socket = socket.clone();
        async move { serve(&state, &socket, 60).await }
    });
    let client = Client::remote(socket.clone()).unwrap();
    for _ in 0..100 {
        if socket.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(
        client.probe().await.unwrap().kind,
        contracts::ServiceKind::Storage
    );
    client.register_operator().await.unwrap();
    client.register(vec!["worker".into()]).await.unwrap();
    client
        .create_group(NewGroup {
            id: "room".into(),
            title: "Room".into(),
            topic: "Remote storage test".into(),
            kind: GroupKind::Conversation,
            private: false,
            members: vec!["operator".into(), "worker".into()],
        })
        .await
        .unwrap();
    let token = client.grant("worker", 60).await.unwrap();
    let mut changes = client.changes().await.unwrap();
    let mut sub = client.subscribe("worker", "room").await.unwrap();
    client
        .set_presence("worker", Presence::Online)
        .await
        .unwrap();
    assert!(sub.receives().await.unwrap());
    client
        .publish(
            &token,
            Publish {
                group_id: "room".into(),
                request_id: "one".into(),
                text: "hello".into(),
                reply_to: None,
            },
        )
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(2), changes.changed())
        .await
        .unwrap()
        .unwrap();
    loop {
        if let Some(message) = tokio::time::timeout(Duration::from_secs(2), sub.recv())
            .await
            .unwrap()
            .unwrap()
        {
            assert_eq!(message.sequence, 1);
            break;
        }
    }
    assert_eq!(client.history("room", 0, 10).await.unwrap().len(), 1);
    let input = GoalInput {
        id: "mention-4d4593b6df2d774271f7c54a33360db22850e8523bbc867d6deaa047c63b9629".into(),
        group_id: "room".into(),
        title: "Mention discussion".into(),
        objective: "@worker review this".into(),
        acceptance: "Provide evidence".into(),
        scenario: "collaboration".into(),
        parameters: json!({}),
        max_seconds: 60,
        assignments: vec![AssignmentInput {
            person_id: "worker".into(),
            instruction: "Discuss".into(),
        }],
    };
    let message = Publish {
        group_id: "room".into(),
        request_id: "discussion".into(),
        text: input.objective.clone(),
        reply_to: Some(1),
    };
    let result = client
        .discuss(input.clone(), message.clone())
        .await
        .unwrap();
    assert_eq!(result["message"]["sequence"], 2);
    assert_eq!(result["goal"]["input"]["id"], input.id);
    assert_eq!(
        client.discuss(input.clone(), message).await.unwrap()["duplicate"],
        true
    );
    let history = client.history("room", 0, 10).await.unwrap();
    assert_eq!(history.len(), 3);
    assert_eq!(history[2].reply_to, Some(2));
    assert!(client.membership("room", "worker", false).await.is_err());
    client
        .change_goal(GoalChange::Cancel {
            goal_id: input.id,
            revision: 1,
        })
        .await
        .unwrap();
    client.set_presence("worker", Presence::Busy).await.unwrap();
    assert!(!sub.receives().await.unwrap());
    client.membership("room", "worker", false).await.unwrap();
    assert!(sub.receives().await.is_err());
    assert!(client.authorize("worker", "room").await.is_err());
    service.abort();
    let _ = service.await;
}

#[tokio::test]
async fn embedded_and_remote_clients_share_delivery_and_idempotency_contracts() {
    let root = tempfile::tempdir().unwrap();
    let socket = root.path().join("private/parity.sock");
    let remote_state = root.path().join("remote");
    let remote = tokio::spawn({
        let socket = socket.clone();
        async move { serve(&remote_state, &socket, 30).await }
    });
    let remote_client = Client::remote(socket.clone()).unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        while remote_client.probe().await.is_err() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let local = client(Hub::open(&root.path().join("embedded")).unwrap());
    for client in [local, remote_client] {
        client.register_operator().await.unwrap();
        client.register(vec!["worker".into()]).await.unwrap();
        client
            .create_group(NewGroup {
                id: "room".into(),
                title: "Room".into(),
                topic: "Compare transports".into(),
                private: false,
                kind: GroupKind::Conversation,
                members: vec!["operator".into(), "worker".into()],
            })
            .await
            .unwrap();
        let token = client.grant("worker", 60).await.unwrap();
        let mut events = client.subscribe("worker", "room").await.unwrap();
        let input = Publish {
            group_id: "room".into(),
            request_id: "same-request".into(),
            text: include_str!("../fixtures/user-content.txt").trim().into(),
            reply_to: None,
        };
        let first = client.publish(&token, input.clone()).await.unwrap();
        assert!(first.durable && !first.duplicate);
        let event = tokio::time::timeout(Duration::from_secs(2), events.recv())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(event.text, input.text);
        assert_eq!(event.sequence, first.message.sequence);
        let repeated = client.publish(&token, input.clone()).await.unwrap();
        assert!(repeated.duplicate);
        assert_eq!(repeated.message.sequence, first.message.sequence);
        let mut conflict = input.clone();
        conflict.text = "Different content".into();
        assert!(client.publish(&token, conflict).await.is_err());
        let history = client.history("room", 0, 10).await.unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].text, input.text);
        client.set_presence("worker", Presence::Busy).await.unwrap();
        assert!(!events.receives().await.unwrap());
        client.membership("room", "worker", false).await.unwrap();
        assert!(events.receives().await.is_err());
        assert!(client.publish(&token, input).await.is_err());
    }
    remote.abort();
    let _ = remote.await;
}

#[tokio::test]
async fn remote_deletion_uses_the_storage_owner_and_returns_structured_results() {
    let root = tempfile::tempdir().unwrap();
    let socket = root.path().join("private/delete.sock");
    let state = root.path().join("db");
    let service = tokio::spawn({
        let socket = socket.clone();
        async move { serve(&state, &socket, 30).await }
    });
    let client = Client::remote(socket.clone()).unwrap();
    for _ in 0..100 {
        if socket.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    client.register_operator().await.unwrap();
    client
        .create_group(NewGroup {
            id: "delete-room".into(),
            title: "Disposable".into(),
            topic: "Deletion".into(),
            kind: GroupKind::Conversation,
            private: false,
            members: vec!["operator".into()],
        })
        .await
        .unwrap();
    let outcome = client
        .delete(DeleteTarget::Group {
            id: "delete-room".into(),
            revision: 1,
        })
        .await
        .unwrap();
    assert_eq!(outcome.code, contracts::DeletionCode::Deleted);
    assert!(outcome.references.is_empty());
    assert!(client.group("delete-room").await.is_err());
    service.abort();
    let _ = service.await;
}
