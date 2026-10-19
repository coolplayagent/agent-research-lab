use super::*;
use std::fs;
#[cfg(unix)]
#[test]
fn reads_reject_symlinks_and_writes_replace_without_following() {
    let d = tempfile::tempdir().unwrap();
    let secret = d.path().join("secret");
    fs::write(&secret, "{\"secret\":1}").unwrap();
    let input = d.path().join("input");
    std::os::unix::fs::symlink(&secret, &input).unwrap();
    assert!(read::<serde_json::Value>(&input).is_err());
    write(&input, &serde_json::json!({"safe":true})).unwrap();
    assert_eq!(fs::read_to_string(secret).unwrap(), "{\"secret\":1}");
    assert_eq!(read::<serde_json::Value>(&input).unwrap()["safe"], true);
}

#[cfg(windows)]
#[test]
fn windows_canonical_paths_support_create_replace_read_and_lock() {
    let root = tempfile::tempdir().unwrap();
    let canonical = fs::canonicalize(root.path()).unwrap();
    assert!(canonical.to_string_lossy().starts_with(r"\\?\"));
    let private = canonical.join("private");
    private_dir(&private).unwrap();
    let path = private.join("services.json");
    write(&path, &serde_json::json!({"revision":0})).unwrap();
    write(&path, &serde_json::json!({"revision":1})).unwrap();
    assert_eq!(read::<serde_json::Value>(&path).unwrap()["revision"], 1);
    private_file(&path).unwrap();
    let _lock = lock(&private.join("owner.lock")).unwrap();
    assert!(lock(&private.join("owner.lock")).is_err());
}

#[cfg(windows)]
#[test]
fn windows_state_is_bounded_and_locked() {
    let d = tempfile::tempdir().unwrap();
    let private = d.path().join("private");
    private_dir(&private).unwrap();
    let path = private.join("state.json");
    write(&path, &serde_json::json!({"ready":true})).unwrap();
    private_file(&path).unwrap();
    assert_eq!(read::<serde_json::Value>(&path).unwrap()["ready"], true);
    fs::write(&path, vec![b'x'; 16 * 1024 * 1024 + 1]).unwrap();
    assert!(read::<serde_json::Value>(&path).is_err());
    let lock_path = private.join("owner.lock");
    let _first = lock(&lock_path).unwrap();
    assert!(lock(&lock_path).is_err());
}
