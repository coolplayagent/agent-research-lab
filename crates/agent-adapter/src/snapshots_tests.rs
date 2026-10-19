use super::*;
#[test]
fn snapshot_rejects_directories_and_oversized_configuration() {
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("copy");
    assert!(copy_file(temp.path(), &target).is_err());
    let source = temp.path().join("config");
    fs::write(&source, vec![0; 1024 * 1024 + 1]).unwrap();
    assert!(copy_file(&source, &target).is_err());
    fs::write(&source, b"private").unwrap();
    copy_file(&source, &target).unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"private");
}
