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
fn windows_state_is_bounded_and_locked() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("state.json");
    write(&path, &serde_json::json!({"ready":true})).unwrap();
    assert_eq!(read::<serde_json::Value>(&path).unwrap()["ready"], true);
    fs::write(&path, vec![b'x'; 16 * 1024 * 1024 + 1]).unwrap();
    assert!(read::<serde_json::Value>(&path).is_err());
    let lock_path = d.path().join("owner.lock");
    let _first = lock(&lock_path).unwrap();
    assert!(lock(&lock_path).is_err());
}
