use super::*;
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
    client.set_presence("worker", Presence::Busy).await.unwrap();
    assert!(!sub.receives().await.unwrap());
    client.membership("room", "worker", false).await.unwrap();
    assert!(sub.receives().await.is_err());
    assert!(client.authorize("worker", "room").await.is_err());
    service.abort();
    let _ = service.await;
}
