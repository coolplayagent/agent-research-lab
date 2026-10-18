use super::*;

#[test]
fn retained_entries_cannot_cross_store_boundaries_or_escape_state_paths() {
    let temp = tempfile::tempdir().unwrap();
    let first = Outbox::open(&temp.path().join("first")).unwrap();
    let second = Outbox::open(&temp.path().join("second")).unwrap();
    let request = Operation::KnowledgeRefresh(knowledge::IndexRefreshRequest {
        relay_knowledge_binary: temp.path().join("missing-cli"),
        knowledge_root: temp.path().into(),
        repository_alias: "knowledge".into(),
        expected_commit: "a".repeat(40),
    });
    let mut entry = first.insert(&request).unwrap();
    let bytes = fs::read(&entry.request_path).unwrap();
    assert!(second.operation(&entry).is_err());
    assert!(second.save(&mut entry).is_err());
    assert!(second.ids().unwrap().is_empty());
    entry.id = "../../escaped".into();
    assert!(first.save(&mut entry).is_err());
    assert_eq!(fs::read(&entry.request_path).unwrap(), bytes);
    assert!(!temp.path().join("escaped.json").exists());
}
