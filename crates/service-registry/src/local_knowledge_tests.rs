use super::*;
#[test]
fn managed_knowledge_paths_are_private_and_external_connections_remain_external() {
    let state = tempfile::tempdir().unwrap();
    let mut config = Configuration::defaults(state.path());
    let mut service = OutcomeService {
        id: "knowledge".into(),
        kind: OutcomeServiceKind::Knowledge,
        endpoint: state.path().join("external.sock"),
        enabled: true,
        management: ServiceManagement::External,
        protocol_version: 1,
        outcome_kinds: OutcomeKind::ALL.to_vec(),
        dependencies: vec![],
        max_seconds: 120,
    };
    config.outcome_services.push(service.clone());
    assert!(validate_locations(&config, state.path()).is_ok());
    service.management = ServiceManagement::LocalManaged;
    config.outcome_services = vec![service.clone()];
    assert!(validate_locations(&config, state.path()).is_err());
    service.endpoint = state.path().join("private/services/knowledge.sock");
    config.outcome_services = vec![service];
    assert!(validate_locations(&config, state.path()).is_ok());
}
