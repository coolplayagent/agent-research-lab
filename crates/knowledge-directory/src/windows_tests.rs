use super::*;
fn target(path: &Path) -> Target {
    Target {
        kind: contracts::KnowledgeBackend::Directory,
        path: fs::canonicalize(path).unwrap(),
        content_root: ".".into(),
        enabled: true,
        access: contracts::KnowledgeAccess::ReadWrite,
        reference: None,
        remote: None,
        credential_env: vec![],
    }
}
#[test]
fn pinned_ancestors_and_reads_deny_replacement_and_concurrent_writers() {
    let temp = tempfile::tempdir().unwrap();
    let nested = temp.path().join("parent/child");
    fs::create_dir_all(&nested).unwrap();
    fs::write(nested.join("note"), "original").unwrap();
    let directory = root(&target(&nested)).unwrap();
    assert!(fs::rename(temp.path().join("parent"), temp.path().join("moved")).is_err());
    let file = open_file(&anchored(&directory).join("note")).unwrap();
    assert!(
        OpenOptions::new()
            .write(true)
            .open(nested.join("note"))
            .is_err()
    );
    assert!(fs::remove_file(nested.join("note")).is_err());
    drop(file);
    fs::write(nested.join("note"), "updated").unwrap();
    drop(directory);
    fs::rename(temp.path().join("parent"), temp.path().join("moved")).unwrap();
}
#[test]
fn junctions_cannot_escape_a_target_or_become_a_target_root() {
    let temp = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("secret"), "private").unwrap();
    let junction = temp.path().join("escape");
    let status = std::process::Command::new("cmd.exe")
        .args(["/D", "/C", "mklink", "/J"])
        .arg(&junction)
        .arg(outside.path())
        .status()
        .unwrap();
    assert!(status.success());
    assert!(crate::inspect(&target(temp.path()), Some(Path::new("escape/secret")), None).is_err());
    let mut forged = target(temp.path());
    forged.path = junction.clone();
    assert!(root(&forged).is_err());
    fs::remove_dir(junction).unwrap();
}
#[test]
fn publication_lock_is_exclusive_and_released_on_drop() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("writer.lock");
    let first = lock(&path).unwrap();
    assert!(lock(&path).is_err());
    drop(first);
    assert!(lock(&path).is_ok());
}
