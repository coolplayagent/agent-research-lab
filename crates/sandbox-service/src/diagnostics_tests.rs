use super::*;
#[test]
fn failure_is_bounded_redacted_and_private() {
    let home = tempfile::tempdir().unwrap();
    fs::create_dir(home.path().join(".codex")).unwrap();
    fs::write(
        home.path().join(".codex/auth.json"),
        r#"{"tokens":{"access_token":"fixture-secret-token"}}"#,
    )
    .unwrap();
    let output = tempfile::NamedTempFile::new().unwrap();
    fs::write(output.path(), format!("request timed out fixture-secret-token fixture-env-secret https://private.example/token?secret=value {}", "x".repeat(80 * 1024))).unwrap();
    let error = failure(
        output.path(),
        home.path(),
        &[("HTTPS_PROXY".into(), "fixture-env-secret".into())],
        Some(1),
    );
    let details = service_protocol::private_details(&error).unwrap();
    assert!(details.len() <= 8192);
    assert!(details.contains("request timed out"));
    for secret in [
        "fixture-secret-token",
        "fixture-env-secret",
        "private.example",
    ] {
        assert!(!details.contains(secret));
    }
    assert!(!format!("{error:#}").contains("request timed out"));
}
