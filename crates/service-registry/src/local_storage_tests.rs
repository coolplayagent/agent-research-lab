use super::*;
fn binary() -> PathBuf {
    std::fs::canonicalize(
        std::env::var_os("TEST_STORAGE_BINARY").expect("run storage lifecycle tests through Bazel"),
    )
    .unwrap()
}
fn service(root: &Path) -> Service {
    service_api::Configuration::local(root)
        .services
        .into_iter()
        .find(|s| s.kind == ServiceKind::Storage)
        .unwrap()
}
#[tokio::test]
async fn owned_storage_recovers_the_same_database_and_rejects_missing_data() {
    let root = tempfile::tempdir().unwrap();
    let service = service(root.path());
    let mut managed = ManagedStorage::default();
    managed
        .prepare(&service, root.path(), &binary())
        .await
        .unwrap();
    let database = root.path().join("private/managed-storage/messages.sqlite");
    assert!(database.is_file());
    let marker = std::fs::read(root.path().join("private/storage-owner-storage.json")).unwrap();
    let mut competitor = ManagedStorage::default();
    assert!(
        competitor
            .prepare(&service, root.path(), &binary())
            .await
            .is_err()
    );
    managed
        .prepare(&service, root.path(), &binary())
        .await
        .unwrap();
    managed.child = None;
    managed.recover().await;
    assert!(managed.child.is_some());
    assert_eq!(
        std::fs::read(root.path().join("private/storage-owner-storage.json")).unwrap(),
        marker
    );
    managed.child = None;
    std::fs::rename(&database, database.with_extension("saved")).unwrap();
    managed.retry_after = None;
    managed.recover().await;
    assert_eq!(managed.error, Some(Code::DataMissing));
    assert!(!database.exists());
}
#[tokio::test]
async fn recovery_is_bounded_and_never_selects_another_store() {
    let root = tempfile::tempdir().unwrap();
    let mut managed = ManagedStorage::default();
    managed
        .prepare(&service(root.path()), root.path(), &binary())
        .await
        .unwrap();
    managed.child = None;
    managed.active.as_mut().unwrap().2 = root.path().join("missing");
    for _ in 0..5 {
        managed.retry_after = None;
        managed.recover().await;
    }
    assert_eq!(managed.attempts, 3);
    assert_eq!(managed.error, Some(Code::RecoveryExhausted));
    assert!(!root.path().join("crystal").exists());
}
#[tokio::test]
async fn occupied_endpoint_and_missing_package_do_not_create_database() {
    let root = tempfile::tempdir().unwrap();
    let service = service(root.path());
    let _listener = service_protocol::Listener::bind(&service.endpoint).unwrap();
    let mut managed = ManagedStorage::default();
    let error = managed
        .launch(&service, root.path(), &binary())
        .await
        .unwrap_err();
    assert_eq!(
        error.downcast_ref::<Failure>().unwrap().0,
        Code::EndpointInUse
    );
    let error = managed
        .launch(&service, root.path(), &root.path().join("missing"))
        .await
        .unwrap_err();
    assert_eq!(
        error.downcast_ref::<Failure>().unwrap().0,
        Code::PackageMissing
    );
    assert!(
        !root
            .path()
            .join("private/managed-storage/messages.sqlite")
            .exists()
    );
}
