use super::*;
fn job() -> Job {
    serde_json::from_value(json!({"task":{"id":"one","role":"research","repository":"superpod","prompt":"independent"},"model":"model","source_commit":"a".repeat(40),"superpod_commit":"b".repeat(40),"prompt_digest":"c".repeat(64),"config_digest":"d".repeat(64),"worktree":"/unused","run_id":"one-attempt-1","attempt":1,"last_error":null,"launch":null})).unwrap()
}
fn team() -> TeamBinding {
    TeamBinding {
        id: "round-one".into(),
        cell: None,
        topics: vec!["sdlc".into()],
        query: String::new(),
    }
}
#[test]
fn host_team_separates_snapshots_and_binds_sender() {
    let a = job();
    let mut b = a.clone();
    let team = team();
    let first = team.member(&a).unwrap();
    b.task.id = "two".into();
    b.run_id = "two-attempt-1".into();
    b.source_commit = "e".repeat(40);
    let second = team.member(&b).unwrap();
    assert_eq!(first.cohort_id, second.cohort_id);
    assert_ne!(first.authority_sha256, second.authority_sha256);
    b.superpod_commit = "f".repeat(40);
    assert_ne!(first.cohort_id, team.member(&b).unwrap().cohort_id);
    b = a.clone();
    b.config_digest = "e".repeat(64);
    assert_ne!(first.cohort_id, team.member(&b).unwrap().cohort_id);
    let mut separate = team.clone();
    separate.id = "holdout".into();
    assert_ne!(first.cohort_id, separate.member(&a).unwrap().cohort_id);
}
#[test]
fn proposals_do_not_become_workflow_success_or_scheduling() {
    let temp = tempfile::tempdir().unwrap();
    let job = job();
    let team = team();
    let prepared = prepare(temp.path(), &job, &team, 100).unwrap();
    storage::write(&prepared.endpoint.outbox.join("00.json"), &json!({"schema_version":1,"id":"claim","kind":"finding","text":"Evidence incomplete; need independent test.","topics":["sdlc"],"references":[],"reply_to":null})).unwrap();
    let result = poll_active(
        temp.path(),
        std::slice::from_ref(&prepared.authority.member),
        101,
    )
    .unwrap();
    assert_eq!(result["automatic_tasks"], 0);
    assert_eq!(result["automatic_wakeups"], 0);
    assert_eq!(result["cohorts"][0]["report"]["accepted"], 1);
    let board = Board::new(temp.path()).unwrap();
    let context = board
        .context(
            &prepared.authority.member,
            &Selection {
                topics: vec!["sdlc".into()],
                query: String::new(),
            },
            101,
        )
        .unwrap();
    assert_eq!(context.message_ids.len(), 1);
    assert!(completed(temp.path(), &job, &team, &json!({}), 102).is_err());
    revoked(temp.path(), &prepared.authority, 102).unwrap();
    revoked(temp.path(), &prepared.authority, 103).unwrap();
    assert!(
        board
            .context(
                &prepared.authority.member,
                &Selection {
                    topics: vec!["sdlc".into()],
                    query: String::new()
                },
                103
            )
            .unwrap()
            .message_ids
            .is_empty()
    );
}
