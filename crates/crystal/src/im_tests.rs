use super::*;
fn group(id: &str, kind: GroupKind, members: &[&str]) -> NewGroup {
    NewGroup {
        id: id.into(),
        title: id.into(),
        topic: "Agent discussion".into(),
        private: kind == GroupKind::Temporary,
        kind,
        members: members.iter().map(|id| (*id).into()).collect(),
    }
}
fn message(group: &str, request: &str, text: &str) -> Publish {
    Publish {
        group_id: group.into(),
        request_id: request.into(),
        text: text.into(),
        reply_to: None,
    }
}
#[tokio::test]
async fn temporary_dm_forks_selected_history_and_converts_without_expiry_or_audience_leak() {
    let state = tempfile::tempdir().unwrap();
    let hub = Hub::open(state.path()).unwrap();
    hub.register(vec!["a".into(), "b".into(), "c".into()])
        .await
        .unwrap();
    hub.create_group(group("dm", GroupKind::Temporary, &["a", "b"]))
        .await
        .unwrap();
    let token = hub.grant("a", 3600).await.unwrap();
    let first = hub
        .publish(&token, message("dm", "one", "original audience only"))
        .await
        .unwrap();
    let mut second = message("dm", "two", "reply");
    second.reply_to = Some(first.message.sequence);
    let second = hub.publish(&token, second).await.unwrap();
    hub.acknowledge(&token, "dm", second.message.sequence)
        .await
        .unwrap();
    assert!(hub.membership("dm", "c", true).await.is_err());
    assert!(hub.authorize("c", "dm").is_err());
    hub.fork(
        "dm",
        group("fresh", GroupKind::Temporary, &["a", "b", "c"]),
        None,
    )
    .await
    .unwrap();
    assert!(hub.history("fresh", 0, 100).await.unwrap().is_empty());
    hub.fork(
        "dm",
        group("copied", GroupKind::Temporary, &["a", "b", "c"]),
        Some(0),
    )
    .await
    .unwrap();
    let copied = hub.history("copied", 0, 100).await.unwrap();
    assert_eq!(copied.len(), 2);
    assert_eq!(copied[0].sender_id, "a");
    assert_eq!(copied[1].reply_to, Some(copied[0].sequence));
    assert_ne!(copied[1].sequence, second.message.sequence);
    assert_eq!(hub.history("dm", 0, 100).await.unwrap().len(), 2);
    hub.fork(
        "dm",
        group("partial", GroupKind::Temporary, &["a", "b", "c"]),
        Some(second.message.sequence),
    )
    .await
    .unwrap();
    let partial = hub.history("partial", 0, 100).await.unwrap();
    assert_eq!(partial.len(), 1);
    assert_eq!(partial[0].reply_to, None);
    assert!(
        hub.fork(
            "dm",
            group("invalid", GroupKind::Temporary, &["a", "c"]),
            None
        )
        .await
        .is_err()
    );
    assert!(hub.group("invalid").is_err());
    let old = hub.group("dm").unwrap();
    let converted = hub
        .convert("dm", old.revision, "Project team".into())
        .await
        .unwrap();
    assert_eq!(converted.kind, GroupKind::Conversation);
    assert!(!converted.private);
    assert_eq!(
        hub.cursor("a", "dm").await.unwrap(),
        second.message.sequence
    );
    assert!(
        hub.convert("dm", old.revision, "stale".into())
            .await
            .is_err()
    );
    hub.membership("dm", "c", true).await.unwrap();
    drop(hub);
    let reopened = Hub::open(state.path()).unwrap();
    assert_eq!(reopened.group("dm").unwrap().title, "Project team");
    assert_eq!(reopened.group("fresh").unwrap().kind, GroupKind::Temporary);
    assert_eq!(
        reopened.history("dm", 0, 100).await.unwrap()[0].text,
        "original audience only"
    );
    assert!(reopened.authorize("c", "dm").is_ok());
    assert!(
        !serde_json::to_string(&reopened.group("fresh").unwrap())
            .unwrap()
            .contains("expir")
    );
}
#[tokio::test]
async fn boards_have_operator_only_publication_and_actor_scoped_discovery() {
    let state = tempfile::tempdir().unwrap();
    let hub = Hub::open(state.path()).unwrap();
    hub.register_operator().await.unwrap();
    hub.save_person(Person {
        id: "a".into(),
        name: "Planner".into(),
        application_id: Some("other-app".into()),
    })
    .await
    .unwrap();
    hub.register(vec!["outsider".into()]).await.unwrap();
    hub.create_group(group("board", GroupKind::Board, &["operator", "a"]))
        .await
        .unwrap();
    let token = hub.grant("a", 3600).await.unwrap();
    let outsider = hub.grant("outsider", 3600).await.unwrap();
    assert!(
        hub.publish(&token, message("board", "announce", "cannot post"))
            .await
            .is_err()
    );
    let mut feed = hub.subscribe("a", "board").unwrap();
    let receipt = hub
        .publish_as(
            "operator",
            message("board", "announcement", "Milestone reached"),
        )
        .await
        .unwrap();
    assert!(receipt.durable);
    assert_eq!(
        feed.recv().await.unwrap().unwrap().text,
        "Milestone reached"
    );
    assert_eq!(
        hub.inbox(&token, "", 100).unwrap()["groups"][0]["id"],
        "board"
    );
    assert_eq!(hub.inbox(&outsider, "", 100).unwrap()["groups"], json!([]));
    assert_eq!(
        hub.people("", "planner", 100).unwrap()["people"][0]["id"],
        "a"
    );
    let before = hub.group("board").unwrap();
    hub.update_group(GroupChange {
        id: "board".into(),
        revision: before.revision,
        title: before.title,
        topic: before.topic,
        pinned: true,
        archived: true,
    })
    .await
    .unwrap();
    assert!(
        hub.publish_as("operator", message("board", "new", "blocked"))
            .await
            .is_err()
    );
    assert!(hub.membership("board", "outsider", true).await.is_err());
    assert_eq!(hub.history("board", 0, 100).await.unwrap().len(), 1);
    drop(feed);
    drop(hub);
    let reopened = Hub::open(state.path()).unwrap();
    assert_eq!(reopened.person("a").unwrap().name, "Planner");
    assert_eq!(reopened.group("board").unwrap().kind, GroupKind::Board);
}
#[tokio::test]
async fn v1_database_migrates_without_replacing_messages_members_or_acknowledgements() {
    let state = tempfile::tempdir().unwrap();
    let db = rusqlite::Connection::open(state.path().join("messages.sqlite")).unwrap();
    db.execute_batch("CREATE TABLE actors(id TEXT PRIMARY KEY,presence TEXT NOT NULL DEFAULT 'online');
        CREATE TABLE groups(id TEXT PRIMARY KEY,title TEXT NOT NULL,topic TEXT NOT NULL,private INTEGER NOT NULL,archived INTEGER NOT NULL DEFAULT 0,pinned INTEGER NOT NULL DEFAULT 0,revision INTEGER NOT NULL DEFAULT 1,created_ms INTEGER NOT NULL,last_sequence INTEGER NOT NULL DEFAULT 0);
        INSERT INTO actors VALUES('legacy','busy');
        INSERT INTO groups VALUES('legacy-room','Old room','old topic',0,0,1,7,1,9);
        CREATE TABLE members(group_id TEXT NOT NULL,actor_id TEXT NOT NULL,cursor INTEGER NOT NULL DEFAULT 0,PRIMARY KEY(group_id,actor_id));
        INSERT INTO members VALUES('legacy-room','legacy',9);
        CREATE TABLE messages(sequence INTEGER PRIMARY KEY AUTOINCREMENT,group_id TEXT NOT NULL,sender_id TEXT NOT NULL,request_id TEXT NOT NULL,text TEXT NOT NULL,reply_to INTEGER,accepted_ms INTEGER NOT NULL,UNIQUE(group_id,sender_id,request_id));
        INSERT INTO messages VALUES(9,'legacy-room','legacy','old-request','Preserved message',NULL,10);").unwrap();
    drop(db);
    let hub = Hub::open(state.path()).unwrap();
    assert_eq!(hub.person("legacy").unwrap().name, "legacy");
    let group = hub.group("legacy-room").unwrap();
    assert_eq!(group.revision, 7);
    assert!(group.pinned);
    assert_eq!(group.kind, GroupKind::Conversation);
    assert_eq!(
        hub.members("legacy-room", "", 10).unwrap()["members"][0]["person_id"],
        "legacy"
    );
    assert_eq!(hub.cursor("legacy", "legacy-room").await.unwrap(), 9);
    assert_eq!(
        hub.history("legacy-room", 0, 10).await.unwrap()[0].text,
        "Preserved message"
    );
}
