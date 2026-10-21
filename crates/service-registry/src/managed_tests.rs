use super::*;
#[test]
fn managed_endpoints_are_scoped_but_external_endpoints_are_preserved() {
    let root = tempfile::tempdir().unwrap();
    let mut config = Configuration::defaults(root.path());
    validate_locations(&config, root.path()).unwrap();
    config.services[0].endpoint = root.path().join("elsewhere.sock");
    assert!(validate_locations(&config, root.path()).is_err());
    config.services[0].management = ServiceManagement::External;
    validate_locations(&config, root.path()).unwrap();
}
#[tokio::test]
async fn missing_packaged_worker_is_observable_without_mutating_configuration() {
    let root = tempfile::tempdir().unwrap();
    let config = Configuration::defaults(root.path());
    let before = serde_json::to_value(&config).unwrap();
    let mut managed = Managed::default();
    assert!(
        managed
            .prepare_with(
                &config,
                root.path(),
                "executor",
                &root.path().join("absent")
            )
            .await
            .is_err()
    );
    assert!(managed.children.is_empty());
    assert_eq!(before, serde_json::to_value(config).unwrap());
}
#[tokio::test]
async fn external_services_are_not_launched_even_when_the_worker_is_absent() {
    let root = tempfile::tempdir().unwrap();
    let mut config = Configuration::defaults(root.path());
    for service in &mut config.services {
        service.management = ServiceManagement::External;
    }
    let mut managed = Managed::default();
    let error = managed
        .prepare_with(
            &config,
            root.path(),
            "executor",
            &root.path().join("absent"),
        )
        .await
        .unwrap_err();
    assert!(!error.to_string().contains("packaged execution service"));
    assert!(managed.children.is_empty());
}
#[cfg(unix)]
#[tokio::test]
async fn early_worker_exit_is_detected_without_waiting_for_the_readiness_deadline() {
    let root = tempfile::tempdir().unwrap();
    let config = Configuration::defaults(root.path());
    let mut managed = Managed::default();
    let result = tokio::time::timeout(
        Duration::from_secs(3),
        managed.prepare_with(&config, root.path(), "executor", Path::new("/bin/false")),
    )
    .await
    .unwrap();
    assert!(result.is_err());
    managed.maintain(&config);
    assert!(managed.children.is_empty());
}
