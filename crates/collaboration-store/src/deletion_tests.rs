use super::*;
use contracts::{SubjectKind, TopicState};
async fn setup(kind: GroupKind) -> (tempfile::TempDir, Hub) {
    let root = tempfile::tempdir().unwrap();
    let hub = Hub::open(root.path()).unwrap();
    hub.register_operator().await.unwrap();
    hub.register(vec!["worker".into()]).await.unwrap();
    hub.create_group(NewGroup {
        id: "group".into(),
        title: "Team".into(),
        topic: "Discussion".into(),
        kind,
        private: kind == GroupKind::Temporary,
        members: vec!["operator".into(), "worker".into()],
    })
    .await
    .unwrap();
    (root, hub)
}
fn group(revision: u64) -> DeleteTarget {
    DeleteTarget::Group {
        id: "group".into(),
        revision,
    }
}
async fn topic(hub: &Hub) {
    hub.research_change(ResearchChange::Topic {
        topic: CollaborationTopic {
            id: "topic".into(),
            group_id: "group".into(),
            title: "Topic".into(),
            objective: "Explore".into(),
            state: TopicState::Active,
            revision: 0,
        },
    })
    .await
    .unwrap();
    hub.research_change(ResearchChange::Subject {
        subject: CollaborationSubject {
            id: "subject".into(),
            topic_id: "topic".into(),
            name: "Subject".into(),
            kind: SubjectKind::Tool,
            reference: "external/tool".into(),
            revision: 0,
        },
    })
    .await
    .unwrap();
}
#[tokio::test]
async fn deletes_group_and_private_chat_durably_with_revision_guard() {
    for kind in [GroupKind::Conversation, GroupKind::Temporary] {
        let (root, hub) = setup(kind).await;
        let receipt = hub
            .publish_as(
                "operator",
                Publish {
                    group_id: "group".into(),
                    request_id: "post".into(),
                    text: "Private content".into(),
                    reply_to: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(
            hub.delete(group(0)).await.unwrap().code,
            DeletionCode::RevisionConflict
        );
        assert_eq!(hub.history("group", 0, 10).await.unwrap().len(), 1);
        assert_eq!(
            hub.delete(group(1)).await.unwrap().code,
            DeletionCode::Deleted
        );
        assert!(hub.group("group").is_err());
        assert!(hub.subscribe("worker", "group").is_err());
        assert_eq!(
            hub.delete(group(1)).await.unwrap().code,
            DeletionCode::NotFound
        );
        assert!(receipt.message.sequence > 0);
        drop(hub);
        let reopened = Hub::open(root.path()).unwrap();
        assert!(reopened.group("group").is_err());
        assert_eq!(reopened.metrics()["stored_messages"], 0);
    }
}
#[tokio::test]
async fn unreferenced_topic_and_subject_are_removed() {
    let (_root, hub) = setup(GroupKind::Conversation).await;
    topic(&hub).await;
    assert_eq!(
        hub.delete(DeleteTarget::Topic {
            id: "topic".into(),
            revision: 1
        })
        .await
        .unwrap()
        .code,
        DeletionCode::Deleted
    );
    assert!(
        hub.topics("", "").await.unwrap()["topics"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(hub.group("group").is_ok());
}
#[tokio::test]
async fn announcement_body_is_erased_without_breaking_reply_or_cursor() {
    let (_root, hub) = setup(GroupKind::Board).await;
    let input = Publish {
        group_id: "group".into(),
        request_id: "announcement".into(),
        text: "Announcement body".into(),
        reply_to: None,
    };
    let mut subscriber = hub.subscribe("worker", "group").unwrap();
    let first = hub
        .publish_as("operator", input.clone())
        .await
        .unwrap()
        .message;
    hub.publish_as(
        "operator",
        Publish {
            request_id: "reply".into(),
            text: "Reply".into(),
            reply_to: Some(first.sequence),
            ..input.clone()
        },
    )
    .await
    .unwrap();
    assert_eq!(
        hub.delete(DeleteTarget::Announcement {
            group_id: "group".into(),
            sequence: first.sequence,
            revision: 1
        })
        .await
        .unwrap()
        .code,
        DeletionCode::Deleted
    );
    assert!(
        subscriber.recv().await.is_err(),
        "Queued content must not escape after deletion"
    );
    let history = hub.history("group", 0, 10).await.unwrap();
    assert!(history[0].deleted);
    assert!(history[0].text.is_empty());
    assert_eq!(history[1].reply_to, Some(first.sequence));
    assert_eq!(hub.group("group").unwrap().revision, 2);
    assert!(
        hub.publish_as("operator", input).await.is_err(),
        "Old request cannot resurrect deleted content"
    );
    assert_eq!(
        hub.delete(group(2)).await.unwrap().code,
        DeletionCode::Unsupported
    );
}
#[tokio::test]
async fn active_goals_and_recorded_results_block_group_deletion() {
    let (_root, hub) = setup(GroupKind::Conversation).await;
    let input = GoalInput {
        id: "goal".into(),
        group_id: "group".into(),
        title: "Task".into(),
        objective: "Answer".into(),
        acceptance: "Host review".into(),
        scenario: "collaboration".into(),
        parameters: json!({}),
        assignments: vec![AssignmentInput {
            person_id: "worker".into(),
            instruction: "Answer independently".into(),
        }],
        max_seconds: 300,
    };
    hub.change_goal(GoalChange::Create { goal: input })
        .await
        .unwrap();
    assert_eq!(
        hub.delete(group(1)).await.unwrap().code,
        DeletionCode::ActiveGoals
    );
    hub.work_as(
        "worker",
        WorkChange::Claim {
            goal_id: "goal".into(),
            work_id: "work-0".into(),
            attempt: 1,
            claim_id: "claim".into(),
        },
    )
    .await
    .unwrap();
    hub.work_as(
        "worker",
        WorkChange::Submit {
            goal_id: "goal".into(),
            work_id: "work-0".into(),
            attempt: 1,
            claim_id: "claim".into(),
            result: WorkResult {
                code: None,
                summary: "Partial answer".into(),
                succeeded: false,
                evidence: json!({"source":"fixture"}),
            },
        },
    )
    .await
    .unwrap();
    let goal = hub.goal("goal").await.unwrap();
    hub.change_goal(GoalChange::Retry {
        goal_id: "goal".into(),
        revision: goal.revision,
        work_id: "work-0".into(),
    })
    .await
    .unwrap();
    let goal = hub.goal("goal").await.unwrap();
    assert!(goal.work[0].result.is_none());
    hub.change_goal(GoalChange::Cancel {
        goal_id: "goal".into(),
        revision: goal.revision,
    })
    .await
    .unwrap();
    assert_eq!(
        hub.delete(group(1)).await.unwrap().code,
        DeletionCode::EvidenceReferences
    );
}
