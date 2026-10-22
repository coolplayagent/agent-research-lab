use super::*;
#[test]
fn unknown_health_never_means_ready() {
    assert_eq!(
        health(&[serde_json::json!({"id":"x","status":"future"})], "x"),
        None
    );
    assert_eq!(local_state(true, false, false), State::Standby);
    assert_eq!(local_state(false, false, false), State::Missing);
    assert_eq!(local_state(true, false, true), State::Failed);
}
#[tokio::test]
async fn public_runtime_status_contains_no_endpoint_or_configuration_contents() {
    let root = tempfile::tempdir().unwrap();
    let registry = Registry::open(root.path()).unwrap();
    let rows = registry
        .runtime_status(&registry.configuration(), &[], &[])
        .await
        .unwrap();
    let value = serde_json::to_string(&rows).unwrap();
    assert_eq!(rows.len(), 3);
    assert!(!value.contains("endpoint"));
    assert!(!value.contains(root.path().to_str().unwrap()));
    assert!(!value.contains("credential"));
}
