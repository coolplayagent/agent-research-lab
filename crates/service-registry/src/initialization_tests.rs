use super::*;

#[test]
fn new_workspace_persists_managed_storage_before_startup() {
    let root = tempfile::tempdir().unwrap();
    let config = load(root.path(), false).unwrap();
    assert_eq!(config.storage_id.as_deref(), Some("storage"));
    assert!(root.path().join("private/services.json").is_file());
    let reopened = load(root.path(), true).unwrap();
    assert_eq!(
        serde_json::to_value(config).unwrap(),
        serde_json::to_value(reopened).unwrap()
    );
}
#[test]
fn old_development_data_and_explicit_development_remain_separate() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("crystal")).unwrap();
    assert!(load(root.path(), false).unwrap().storage_id.is_none());
    let fresh = tempfile::tempdir().unwrap();
    assert!(load(fresh.path(), true).unwrap().storage_id.is_none());
}
#[test]
fn external_configuration_and_disabled_services_survive_byte_for_byte() {
    let root = tempfile::tempdir().unwrap();
    let mut config = Configuration::defaults(root.path());
    for service in &mut config.services {
        service.management = contracts::ServiceManagement::External;
        service.enabled = false;
    }
    let path = root.path().join("private/services.json");
    storage::write(&path, &config).unwrap();
    let before = std::fs::read(&path).unwrap();
    let loaded = load(root.path(), false).unwrap();
    assert!(loaded.storage_id.is_none());
    assert_eq!(std::fs::read(&path).unwrap(), before);
}
#[test]
fn missing_configuration_never_reinitializes_known_managed_data() {
    let root = tempfile::tempdir().unwrap();
    storage::private_dir(&root.path().join("private/managed-storage")).unwrap();
    assert!(load(root.path(), false).is_err());
    assert!(!root.path().join("private/services.json").exists());
}
