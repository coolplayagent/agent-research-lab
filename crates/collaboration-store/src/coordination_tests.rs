use super::*;
use contracts::{SubjectKind, TopicState};
async fn setup() -> (tempfile::TempDir, Hub) {
    let root = tempfile::tempdir().unwrap();
    let hub = Hub::open(root.path()).unwrap();
    hub.register_operator().await.unwrap();
    hub.register(vec!["worker".into()]).await.unwrap();
    hub.create_group(NewGroup {
        id: "group".into(),
        title: "Research".into(),
        topic: "Research coordination".into(),
        kind: GroupKind::Conversation,
        private: false,
        members: vec!["operator".into(), "worker".into()],
    })
    .await
    .unwrap();
    hub.research_change(ResearchChange::Topic {
        topic: CollaborationTopic {
            id: "topic".into(),
            group_id: "group".into(),
            title: "Question".into(),
            objective: "Compare versions".into(),
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
            name: "Tool".into(),
            kind: SubjectKind::Tool,
            reference: "repository/tool".into(),
            revision: 0,
        },
    })
    .await
    .unwrap();
    (root, hub)
}
#[tokio::test]
async fn topics_and_subjects_keep_revision_fences_and_archival() {
    let (root, hub) = setup().await;
    let mut topic: CollaborationTopic = serde_json::from_value(
        hub.topics("", "Question").await.unwrap()["topics"][0]["topic"].clone(),
    )
    .unwrap();
    let mut subject: CollaborationSubject = serde_json::from_value(
        hub.subjects("topic", "").await.unwrap()["subjects"][0]["subject"].clone(),
    )
    .unwrap();
    let mut stale = topic.clone();
    stale.revision = 0;
    assert!(
        hub.research_change(ResearchChange::Topic { topic: stale })
            .await
            .is_err()
    );
    topic.state = TopicState::Archived;
    hub.research_change(ResearchChange::Topic { topic })
        .await
        .unwrap();
    subject.name = "Changed".into();
    assert!(
        hub.research_change(ResearchChange::Subject { subject })
            .await
            .is_err()
    );
    drop(hub);
    let hub = Hub::open(root.path()).unwrap();
    assert_eq!(
        hub.subjects("topic", "").await.unwrap()["subjects"][0]["subject"]["name"],
        "Tool"
    );
}
