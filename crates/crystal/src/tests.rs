use super::*;
fn input(request: &str) -> Publish {
    Publish {
        group_id: "research".into(),
        request_id: request.into(),
        text: "I would test the counterexample first: does it change our shared question?".into(),
        reply_to: None,
    }
}
async fn setup(hub: &Hub) -> String {
    hub.register(vec!["lumen".into(), "cedar".into(), "outsider".into()])
        .await
        .unwrap();
    hub.create_group(NewGroup {
        id: "research".into(),
        title: "Joint research".into(),
        topic: "Test whether collaboration produces new hypotheses".into(),
        private: true,
        kind: Default::default(),
        members: vec!["lumen".into(), "cedar".into()],
    })
    .await
    .unwrap();
    hub.grant("lumen", 3600).await.unwrap()
}
#[tokio::test]
async fn acknowledgements_survive_restart_and_conflicting_retries_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let hub = Hub::open(dir.path()).unwrap();
    assert!(Hub::open(dir.path()).is_err());
    let token = setup(&hub).await;
    let receipt = hub.publish(&token, input("first")).await.unwrap();
    assert!(receipt.durable && !receipt.duplicate);
    drop(hub);
    let hub = Hub::open(dir.path()).unwrap();
    assert_eq!(
        hub.history("research", 0, 128).await.unwrap()[0].sequence,
        receipt.message.sequence
    );
    let retry = hub.publish(&token, input("first")).await.unwrap();
    assert!(retry.duplicate);
    let mut changed = input("first");
    changed.text = "different".into();
    assert!(hub.publish(&token, changed).await.is_err());
    hub.acknowledge(&token, "research", receipt.message.sequence)
        .await
        .unwrap();
    hub.acknowledge(&token, "research", 0).await.unwrap();
    assert_eq!(
        hub.cursor("lumen", "research").await.unwrap(),
        receipt.message.sequence
    );
    assert!(
        hub.acknowledge(&token, "research", receipt.message.sequence + 1)
            .await
            .is_err()
    );
    assert_eq!(hub.metrics()["stored_messages"], 1);
}
#[tokio::test]
async fn membership_rotation_presence_and_replay_have_enforced_boundaries() {
    let dir = tempfile::tempdir().unwrap();
    let hub = Hub::open(dir.path()).unwrap();
    let token = setup(&hub).await;
    let outsider = hub.grant("outsider", 3600).await.unwrap();
    assert!(hub.publish(&outsider, input("bad")).await.is_err());
    assert!(hub.subscribe("outsider", "research").is_err());
    assert!(hub.membership("research", "outsider", true).await.is_err());
    let mut sub = hub.subscribe("cedar", "research").unwrap();
    hub.set_presence("cedar", Presence::Busy).await.unwrap();
    assert!(!sub.receives().unwrap());
    for n in 0..LIVE_CAPACITY + 5 {
        hub.publish(&token, input(&format!("m-{n}"))).await.unwrap();
    }
    hub.set_presence("cedar", Presence::Online).await.unwrap();
    assert!(sub.receives().unwrap());
    assert!(sub.recv().await.unwrap().is_none()); // control or explicit ring lag
    let mut cursor = 0;
    let mut count = 0;
    loop {
        let page = hub.history("research", cursor, 128).await.unwrap();
        if page.is_empty() {
            break;
        }
        count += page.len();
        cursor = page.last().unwrap().sequence;
    }
    assert_eq!(count, LIVE_CAPACITY + 5);
    hub.membership("research", "cedar", false).await.unwrap();
    assert!(sub.receives().is_err());
    let new_token = hub.grant("lumen", 3600).await.unwrap();
    assert!(hub.publish(&token, input("revoked")).await.is_err());
    hub.publish(&new_token, input("allowed")).await.unwrap();
    drop(sub);
    assert_eq!(hub.presence("cedar").unwrap().presence, Presence::Offline);
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_retries_commit_once_and_publish_one_live_event() {
    let dir = tempfile::tempdir().unwrap();
    let hub = Hub::open(dir.path()).unwrap();
    let token = setup(&hub).await;
    let mut sub = hub.subscribe("cedar", "research").unwrap();
    let mut tasks = Vec::new();
    for _ in 0..100 {
        let hub = hub.clone();
        let token = token.clone();
        tasks.push(tokio::spawn(async move {
            hub.publish(&token, input("same-request")).await.unwrap()
        }));
    }
    let mut originals = 0;
    for task in tasks {
        originals += usize::from(!task.await.unwrap().duplicate);
    }
    assert_eq!(originals, 1);
    assert_eq!(hub.history("research", 0, 128).await.unwrap().len(), 1);
    assert_eq!(
        sub.recv().await.unwrap().unwrap().request_id,
        "same-request"
    );
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(20), sub.recv())
            .await
            .is_err()
    );
}
#[tokio::test]
async fn control_transactions_roll_back_and_archives_keep_readable_history() {
    let dir = tempfile::tempdir().unwrap();
    let hub = Hub::open(dir.path()).unwrap();
    let token = setup(&hub).await;
    assert!(
        hub.create_group(NewGroup {
            id: "invalid".into(),
            title: "t".into(),
            topic: "t".into(),
            private: false,
            kind: Default::default(),
            members: vec!["missing".into()]
        })
        .await
        .is_err()
    );
    assert!(hub.group("invalid").is_err());
    let receipt = hub.publish(&token, input("before")).await.unwrap();
    let change = GroupChange {
        id: "research".into(),
        revision: 1,
        title: "Archive".into(),
        topic: "Preserve history".into(),
        archived: true,
        pinned: true,
    };
    hub.update_group(change.clone()).await.unwrap();
    assert!(hub.update_group(change).await.is_err());
    assert!(hub.publish(&token, input("after")).await.is_err());
    assert!(
        hub.publish(&token, input("before"))
            .await
            .unwrap()
            .duplicate
    );
    assert_eq!(
        hub.history("research", 0, 128).await.unwrap()[0].sequence,
        receipt.message.sequence
    );
}

#[tokio::test]
async fn storage_write_failure_never_acknowledges_and_reopen_recovers_authority() {
    let dir = tempfile::tempdir().unwrap();
    let hub = Hub::open(dir.path()).unwrap();
    let token = setup(&hub).await;
    hub.publish(&token, input("committed")).await.unwrap();
    let mut invalid = input("invalid-number");
    invalid.reply_to = Some(u64::MAX);
    assert!(hub.publish(&token, invalid).await.is_err());
    assert_eq!(hub.metrics()["healthy"], true);
    let mut subscriber = hub.subscribe("cedar", "research").unwrap();
    hub.control(Control::ReadOnlyFault).await.unwrap();
    assert!(hub.publish(&token, input("uncertain")).await.is_err());
    assert_eq!(hub.metrics()["healthy"], false);
    tokio::time::timeout(
        std::time::Duration::from_millis(100),
        subscriber.wait_control(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(subscriber.receives().is_err());
    drop(subscriber);
    assert!(hub.publish(&token, input("must-not-ack")).await.is_err());
    drop(hub);
    let hub = Hub::open(dir.path()).unwrap();
    let history = hub.history("research", 0, 128).await.unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].request_id, "committed");
    assert!(
        !hub.publish(&token, input("uncertain"))
            .await
            .unwrap()
            .duplicate
    );
}
#[tokio::test]
async fn pinning_orders_all_pages_and_revision_conflicts_cannot_overwrite() {
    let dir = tempfile::tempdir().unwrap();
    let hub = Hub::open(dir.path()).unwrap();
    setup(&hub).await;
    for id in ["alpha", "zulu"] {
        hub.create_group(NewGroup {
            id: id.into(),
            title: id.into(),
            topic: "t".into(),
            private: false,
            kind: Default::default(),
            members: vec!["lumen".into()],
        })
        .await
        .unwrap();
    }
    hub.update_group(GroupChange {
        id: "zulu".into(),
        revision: 1,
        title: "Z".into(),
        topic: "t".into(),
        archived: false,
        pinned: true,
    })
    .await
    .unwrap();
    let first = hub.groups("", "", 1).unwrap();
    assert_eq!(first["groups"][0]["id"], "zulu");
    let second = hub
        .groups(first["next_after"].as_str().unwrap(), "", 1)
        .unwrap();
    assert_eq!(second["groups"][0]["id"], "alpha");
}

#[tokio::test]
async fn control_storage_failure_also_stops_delivery_authority() {
    let root = tempfile::tempdir().unwrap();
    let hub = Hub::open(root.path()).unwrap();
    hub.register(vec!["a".into()]).await.unwrap();
    hub.control(Control::ReadOnlyFault).await.unwrap();
    assert!(hub.register(vec!["b".into()]).await.is_err());
    assert_eq!(hub.metrics()["healthy"], false);
    assert!(hub.grant("a", 60).await.is_err());
    drop(hub);
    let recovered = Hub::open(root.path()).unwrap();
    assert!(recovered.grant("a", 60).await.is_ok());
    assert!(recovered.grant("b", 60).await.is_err());
}

#[tokio::test]
async fn oversized_history_cursor_rejects_without_disabling_storage() {
    let root = tempfile::tempdir().unwrap();
    let hub = Hub::open(root.path()).unwrap();
    let token = setup(&hub).await;
    assert!(hub.history("research", u64::MAX, 1).await.is_err());
    assert_eq!(hub.metrics()["healthy"], true);
    assert!(
        hub.publish(&token, input("after-invalid-cursor"))
            .await
            .unwrap()
            .durable
    );
}
