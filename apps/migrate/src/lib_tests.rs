use super::*;
use contracts::{FollowupAdmission, MemoryWriteback};
use rusqlite::Connection;
use serde_json::{Value, json};
fn storage_source(root: &Path, version: u64) -> std::path::PathBuf {
    let source = root.join("source");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("writer.lock"), "").unwrap();
    let db = Connection::open(source.join("messages.sqlite")).unwrap();
    database::fixture_version(&db, version).unwrap();
    db.execute(
        "INSERT INTO actors(id,presence) VALUES('person','busy')",
        [],
    )
    .unwrap();
    db.execute("INSERT INTO groups(id,title,topic,private,revision,created_ms,last_sequence) VALUES('room','Room','Topic',0,7,1,9)", []).unwrap();
    db.execute("INSERT INTO members VALUES('room','person',9)", [])
        .unwrap();
    db.execute("INSERT INTO messages(sequence,group_id,sender_id,request_id,text,accepted_ms) VALUES(9,'room','person','req',?1,10)", [include_str!("../tests/fixtures/message.txt")]).unwrap();
    drop(db);
    source
}
#[tokio::test]
async fn every_storage_version_preserves_messages_identity_and_cursors() {
    for version in 1..=7 {
        let temp = tempfile::tempdir().unwrap();
        let source = storage_source(temp.path(), version);
        let before = inventory(&source).unwrap();
        let target = temp.path().join("target");
        inspect(&source).unwrap();
        let report = convert(&source, &target).unwrap();
        assert_eq!(report.source_files, before);
        verify(&source, &target).unwrap();
        assert_eq!(inventory(&source).unwrap(), before);
        let hub = collaboration_store::Hub::open(&target).unwrap();
        assert_eq!(hub.group("room").unwrap().revision, 7);
        assert_eq!(hub.cursor("person", "room").await.unwrap(), 9);
        assert_eq!(
            hub.history("room", 0, 10).await.unwrap()[0].text,
            include_str!("../tests/fixtures/message.txt")
        );
        if version == 1 {
            assert_eq!(hub.person("person").unwrap().name, "person");
        }
        drop(hub);
        assert!(convert(&source, &target).is_err());
        assert!(convert(&target, &temp.path().join("again")).is_err());
    }
}
fn record_source(root: &Path) -> std::path::PathBuf {
    let source = root.join("source");
    fs::create_dir_all(source.join("jobs")).unwrap();
    fs::create_dir_all(source.join("dashboard")).unwrap();
    fs::write(
        source.join("jobs/legacy-task.json"),
        include_str!("../tests/fixtures/serde-abe6720/legacy-job.serialized.json"),
    )
    .unwrap();
    fs::write(
        source.join("dashboard/personas.json"),
        include_str!("../tests/fixtures/personas.json"),
    )
    .unwrap();
    storage::write(
        &source.join("workflow/host-followups/legacy-task-attempt-1.json"),
        &json!({"task_id":"legacy-task","run_id":"legacy-task-attempt-1"}),
    )
    .unwrap();
    storage::write(&source.join("runs/legacy-task-attempt-1/receipt.json"), &json!({"source_commit":"original-evidence","agent_report":{"summary":"Historical result"}})).unwrap();
    source
}
#[test]
fn identities_and_frozen_bytes_survive_without_replaying_unknown_effects() {
    let temp = tempfile::tempdir().unwrap();
    let source = record_source(temp.path());
    let target = temp.path().join("target");
    convert(&source, &target).unwrap();
    let directory: runtime::people::Directory =
        storage::read(&target.join("people/directory.json")).unwrap();
    let names: BTreeMap<String, String> =
        serde_json::from_str(include_str!("../tests/fixtures/personas.json")).unwrap();
    assert_eq!(
        directory.people[&directory.tasks["legacy-task"]].name,
        names["legacy-task"]
    );
    for file in [
        "jobs/legacy-task.json",
        "runs/legacy-task-attempt-1/receipt.json",
    ] {
        assert_eq!(
            fs::read(source.join(file)).unwrap(),
            fs::read(target.join(file)).unwrap()
        );
    }
    assert_eq!(
        fs::read(source.join("dashboard/personas.json")).unwrap(),
        fs::read(target.join("migration-originals/dashboard/personas.json")).unwrap()
    );
    let record: task::PostSuccess =
        storage::read(&target.join("workflow/host-followups/legacy-task-attempt-1.json")).unwrap();
    assert_eq!(record.memory, MemoryWriteback::Unknown);
    assert_eq!(record.persona_memory, MemoryWriteback::Unknown);
    assert_eq!(record.followups, FollowupAdmission::Blocked);
    verify(&source, &target).unwrap();
    fs::write(target.join("jobs/legacy-task.json"), "{}").unwrap();
    assert!(verify(&source, &target).is_err());
}
#[test]
fn invalid_database_and_journal_fail_without_publishing_or_mutating_source() {
    let temp = tempfile::tempdir().unwrap();
    let source = storage_source(temp.path(), 7);
    let db = Connection::open(source.join("messages.sqlite")).unwrap();
    db.execute("UPDATE crystal_meta SET value=99", []).unwrap();
    drop(db);
    let before = inventory(&source).unwrap();
    let target = temp.path().join("target");
    assert!(convert(&source, &target).is_err());
    assert!(!target.exists());
    assert_eq!(before, inventory(&source).unwrap());
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
    let temp = tempfile::tempdir().unwrap();
    let source = record_source(temp.path());
    storage::write(&source.join("workflow/host-followups/legacy-task-attempt-1.json"), &json!({"schema_version":1,"task_id":"legacy-task","run_id":"legacy-task-attempt-1","memory":"unexpected"})).unwrap();
    assert!(convert(&source, &temp.path().join("target")).is_err());
    assert!(!temp.path().join("target").exists());
}
#[test]
fn live_store_symlink_and_nested_destination_are_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let source = storage_source(temp.path(), 7);
    let owner = storage::lock(&source.join("writer.lock")).unwrap();
    assert!(convert(&source, &temp.path().join("target")).is_err());
    drop(owner);
    assert!(convert(&source, &source.join("target")).is_err());
    std::os::unix::fs::symlink("messages.sqlite", source.join("alias")).unwrap();
    assert!(inspect(&source).is_err());
}
#[test]
fn old_directory_conflicts_and_unknown_fields_are_not_silently_dropped() {
    let temp = tempfile::tempdir().unwrap();
    let source = record_source(temp.path());
    storage::write(
        &source.join("dashboard/personas.json"),
        &json!({"legacy-task":"duplicate", "other":"duplicate"}),
    )
    .unwrap();
    assert!(convert(&source, &temp.path().join("target")).is_err());
    let _: Value = storage::read(&source.join("jobs/legacy-task.json")).unwrap();
}
