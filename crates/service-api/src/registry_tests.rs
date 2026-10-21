use super::*;
#[test]
fn default_services_are_local_execution_without_unused_storage() {
    let config = Configuration::defaults(Path::new("/tmp/outcome-hub"));
    assert_eq!(config.services.len(), 2);
    assert!(
        config
            .services
            .iter()
            .all(|s| s.enabled && s.management == contracts::ServiceManagement::LocalManaged)
    );
    assert!(
        config
            .services
            .iter()
            .all(|s| s.kind != ServiceKind::Storage)
    );
    assert!(config.storage_id.is_none());
    config.validate().unwrap();
}
#[test]
fn legacy_service_management_defaults_to_external_and_unknown_modes_fail() {
    let config = Configuration::defaults(Path::new("/tmp/outcome-hub"));
    let mut value = serde_json::to_value(&config.services[0]).unwrap();
    value.as_object_mut().unwrap().remove("management");
    let legacy: Service = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(legacy.management, contracts::ServiceManagement::External);
    value["management"] = serde_json::json!("future");
    assert!(serde_json::from_value::<Service>(value).is_err());
}
