use super::*;
#[test]
fn canonical_runtime_paths_keep_the_same_file_without_a_verbatim_prefix() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("runtime file.js");
    fs::write(&file, b"fixture").unwrap();
    let canonical = fs::canonicalize(file).unwrap();
    let ordinary = user_path(&canonical).unwrap();
    assert!(
        matches!(ordinary.components().next(), Some(Component::Prefix(prefix)) if matches!(prefix.kind(), Prefix::Disk(_)))
    );
    assert_eq!(fs::canonicalize(&ordinary).unwrap(), canonical);
    assert_eq!(fs::read(ordinary).unwrap(), b"fixture");
}
