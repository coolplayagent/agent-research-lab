use super::*;
use std::collections::BTreeSet;

#[tokio::test]
async fn cleanup_keeps_bound_people_and_historical_messages() {
    let state = tempfile::tempdir().unwrap();
    let hub = Hub::open(state.path()).unwrap();
    hub.register_operator().await.unwrap();
    for id in ["bound", "old"] {
        hub.save_person(Person {
            id: id.into(),
            name: id.into(),
            application_id: None,
        })
        .await
        .unwrap();
    }
    hub.create_group(NewGroup {
        id: "history".into(),
        title: "History".into(),
        topic: "Past work".into(),
        private: true,
        kind: GroupKind::Conversation,
        members: vec!["bound".into(), "old".into()],
    })
    .await
    .unwrap();
    let token = hub.grant("old", 3600).await.unwrap();
    hub.publish(
        &token,
        Publish {
            group_id: "history".into(),
            request_id: "past".into(),
            text: "A historical message".into(),
            reply_to: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        hub.archive_unbound(BTreeSet::from(["bound".into()]))
            .await
            .unwrap(),
        1
    );
    let page = hub.people("", "", 100).unwrap();
    assert_eq!(page["total"], 1);
    assert_eq!(page["people"][0]["id"], "bound");
    assert!(hub.authenticate(&token).is_err());
    assert_eq!(
        hub.history("history", 0, 100).await.unwrap()[0].sender_id,
        "old"
    );
    assert_eq!(
        hub.archive_unbound(BTreeSet::from(["bound".into()]))
            .await
            .unwrap(),
        0
    );
    drop(hub);
    let hub = Hub::open(state.path()).unwrap();
    assert_eq!(hub.people("", "", 100).unwrap()["total"], 1);
    hub.save_person(Person {
        id: "old".into(),
        name: "Restored".into(),
        application_id: None,
    })
    .await
    .unwrap();
    assert_eq!(hub.people("", "", 100).unwrap()["total"], 2);
}
