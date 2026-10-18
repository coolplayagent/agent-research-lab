use super::*;
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
