use super::*;
#[test]
fn default_services_are_local_execution_without_unused_storage() {
    let root = tempfile::tempdir().unwrap();
    let config = Configuration::defaults(root.path());
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
    let root = tempfile::tempdir().unwrap();
    let config = Configuration::defaults(root.path());
    let mut value = serde_json::to_value(&config.services[0]).unwrap();
    value.as_object_mut().unwrap().remove("management");
    let legacy: Service = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(legacy.management, contracts::ServiceManagement::External);
    value["management"] = serde_json::json!("future");
    assert!(serde_json::from_value::<Service>(value).is_err());
}

#[test]
fn legacy_knowledge_connections_preserve_wire_bytes_and_remain_external() {
    let value = serde_json::json!({"id":"knowledge","kind":"knowledge","endpoint":"/private/knowledge.sock","enabled":true,"protocol_version":1,"outcome_kinds":["finding"],"dependencies":[],"max_seconds":120});
    let service: crate::OutcomeService = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(service.management, contracts::ServiceManagement::External);
    assert_eq!(serde_json::to_value(service).unwrap(), value);
}
