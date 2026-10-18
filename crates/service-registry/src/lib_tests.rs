use super::*;
#[test]
fn configuration_is_revision_checked_and_dependencies_are_typed() {
    let root = tempfile::tempdir().unwrap();
    let registry = Registry::open(root.path()).unwrap();
    let initial = registry.configuration();
    let mut next = initial.clone();
    next.storage_id = Some("sandbox".into());
    assert!(registry.save(next).is_err());
    let mut next = initial.clone();
    next.adapters[0].enabled = true;
    assert!(registry.save(next).is_err());
    registry.save(initial.clone()).unwrap();
    assert!(registry.save(initial).is_err());
    assert_eq!(
        Registry::open(root.path())
            .unwrap()
            .configuration()
            .revision,
        1
    );
}
#[test]
fn scenario_configuration_is_optional_versioned_and_dependency_checked() {
    let root = tempfile::tempdir().unwrap();
    let mut config = Configuration::defaults(root.path());
    let mut legacy = serde_json::to_value(&config).unwrap();
    legacy.as_object_mut().unwrap().remove("scenarios");
    assert!(
        serde_json::from_value::<Configuration>(legacy)
            .unwrap()
            .scenarios
            .is_empty()
    );
    config.scenarios.push(ScenarioRegistration {
        id: "planning".into(),
        endpoint: root.path().join("private/planning.sock"),
        enabled: true,
        protocol_version: 1,
        capabilities: vec![contracts::ScenarioCapability::Execute],
        dependencies: vec!["executor".into()],
    });
    config.validate().unwrap();
    config.scenarios[0].protocol_version = 2;
    assert!(config.validate().is_err());
    config.scenarios[0].protocol_version = 1;
    config.scenarios[0].dependencies = vec!["missing".into()];
    assert!(config.validate().is_err());
    config.scenarios[0].dependencies = vec![];
    config.scenarios.push(config.scenarios[0].clone());
    assert!(config.validate().is_err());
}
