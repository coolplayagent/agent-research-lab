use super::*;
#[test]
fn atomic_replacement_respects_open_readers_and_keeps_the_parent_pinned() {
    let temp = tempfile::tempdir().unwrap();
    let directory = super::super::root(&knowledge_model::Target {
        kind: contracts::KnowledgeBackend::Directory,
        path: fs::canonicalize(temp.path()).unwrap(),
        content_root: ".".into(),
        enabled: true,
        access: contracts::KnowledgeAccess::ReadWrite,
        reference: None,
        remote: None,
        credential_env: vec![],
    })
    .unwrap();
    let name = Path::new("record.json");
    write(&directory, name, &"original").unwrap();
    let reader = super::super::open_file(&temp.path().join(name)).unwrap();
    assert!(write(&directory, name, &"replacement").is_err());
    assert_eq!(
        storage::read::<String>(&temp.path().join(name)).unwrap(),
        "original"
    );
    drop(reader);
    write(&directory, name, &"replacement").unwrap();
    assert_eq!(
        storage::read::<String>(&temp.path().join(name)).unwrap(),
        "replacement"
    );
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
    assert!(write(&directory, Path::new("../escaped.json"), &"escape").is_err());
}
