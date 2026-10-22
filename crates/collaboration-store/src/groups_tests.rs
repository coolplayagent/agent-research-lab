use super::*;
use crate::Hub;
#[tokio::test]
async fn active_outcome_tasks_keep_their_conversation_and_host_available() {
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
    hub.tasks(TaskCommand::Create {
        input: TaskInput {
            id: "task".into(),
            group_id: "team".into(),
            intent: "Deliver".into(),
            budget: TaskBudget::default(),
        },
    })
    .await
    .unwrap();
    let archive = GroupChange {
        id: "team".into(),
        title: "Team".into(),
        topic: "Work".into(),
        archived: true,
        pinned: false,
        revision: 1,
    };
    assert!(hub.update_group(archive.clone()).await.is_err());
    assert!(hub.membership("team", "operator", false).await.is_err());
    hub.tasks(TaskCommand::Change {
        id: "task".into(),
        revision: 1,
        action: TaskAction::Cancel,
    })
    .await
    .unwrap();
    hub.update_group(archive).await.unwrap();
}
