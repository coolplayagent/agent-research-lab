use crate::{
    Board,
    tests::{member, proposal, selection, write},
};
use native_effects::fs;
use serde_json::{Value, json};

#[test]
fn projections_reject_corrupt_authority_without_repairing_or_republishing_it() {
    let temp = tempfile::tempdir().unwrap();
    let board = Board::new(temp.path()).unwrap();
    let sender = member(1, "task", "run");
    let endpoint = board.register(&sender, 100).unwrap();
    write(&endpoint, 0, &proposal("p1"));
    assert_eq!(board.poll(&sender.cohort_id, 101).unwrap().accepted, 1);
    let path = board.store.state_path(&sender.cohort_id);
    let original = fs::read(&path).unwrap();
    let state: Value = serde_json::from_slice(&original).unwrap();
    let view = fs::read(endpoint.view.join("board.json")).unwrap();
    let mut invalid_audience = state["messages"][0]["proposal"].clone();
    invalid_audience["recipients"] = json!(["unregistered"]);
    for (pointer, replacement) in [
        ("/schema_version", json!(2)),
        ("/members/0/status", json!("future_origin_state")),
        (
            "/members/0/identity/authority_sha256",
            json!("b".repeat(64)),
        ),
        ("/members/0/accepted", json!(0)),
        ("/members/0/inbox/state", json!("future_presence_state")),
        ("/members/0/inbox/delivered", json!([999])),
        ("/messages/0/expires_at", json!(9000)),
        ("/messages/0/id", json!("c".repeat(64))),
        ("/messages/0/proposal_sha256", json!("d".repeat(64))),
        ("/messages/0/reply_depth", json!(1)),
        ("/messages/0/proposal", invalid_audience),
    ] {
        let mut changed = state.clone();
        *changed.pointer_mut(pointer).unwrap() = replacement;
        storage::write(&path, &changed).unwrap();
        let corrupted = fs::read(&path).unwrap();
        assert!(board.view(&sender.cohort_id, 102).is_err(), "{pointer}");
        assert!(board.inbox(&sender, 102).is_err(), "{pointer}");
        assert!(
            board.context(&sender, &selection(), 102).is_err(),
            "{pointer}"
        );
        assert_eq!(fs::read(&path).unwrap(), corrupted, "{pointer}");
        assert_eq!(
            fs::read(endpoint.view.join("board.json")).unwrap(),
            view,
            "{pointer}"
        );
    }
    fs::write(&path, &original).unwrap();
    assert_eq!(board.inbox(&sender, 102).unwrap()["accepted_total"], 1);
}
