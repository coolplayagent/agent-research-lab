use super::*;
use service_api::KnowledgeTarget;
use service_api::OutcomeService;
fn service(
    id: &str,
    kind: OutcomeServiceKind,
    management: ServiceManagement,
    state: &Path,
) -> OutcomeService {
    OutcomeService {
        id: id.into(),
        kind,
        management,
        enabled: true,
        endpoint: state.join(format!("private/services/{id}.sock")),
        protocol_version: service_api::OUTCOME_PROTOCOL_VERSION,
        outcome_kinds: contracts::OutcomeKind::ALL.to_vec(),
        dependencies: vec![],
        max_seconds: 120,
    }
}
#[test]
fn migration_removes_only_external_knowledge_connections_and_their_current_bindings() {
    let directory = tempfile::tempdir().unwrap();
    let state = std::fs::canonicalize(directory.path()).unwrap();
    let mut config = Configuration::defaults(&state);
    config.outcome_services = vec![
        service(
            "external",
            OutcomeServiceKind::Knowledge,
            ServiceManagement::External,
            &state,
        ),
        service(
            "local",
            OutcomeServiceKind::Knowledge,
            ServiceManagement::LocalManaged,
            &state,
        ),
    ];
    let binding = |service: &str| KnowledgeTarget {
        service_id: service.into(),
        repository: "notes".into(),
    };
    config
        .knowledge_groups
        .insert("group".into(), binding("external"));
    config
        .knowledge_goals
        .insert("goal".into(), binding("local"));
    let path = state.join("private/services.json");
    storage::write(&path, &config).unwrap();
    let original = serde_json::to_value(&config).unwrap();
    migrate(&path, &mut config).unwrap();
    assert_eq!(config.revision, 1);
    assert_eq!(config.outcome_services.len(), 1);
    assert!(config.knowledge_groups.is_empty());
    assert_eq!(config.knowledge_goals.len(), 1);
    let summary: serde_json::Value =
        storage::read(&path.with_file_name("knowledge-external-migration.json")).unwrap();
    let backup: serde_json::Value =
        storage::read(&path.with_file_name(summary["backup"].as_str().unwrap())).unwrap();
    assert_eq!(backup, original);
    migrate(&path, &mut config).unwrap();
    assert_eq!(config.revision, 1);
    let reopened = crate::Registry::open(&state).unwrap();
    assert_eq!(reopened.configuration().revision, 1);
}

#[test]
fn new_external_knowledge_connections_are_rejected_without_mutating_settings() {
    let state = tempfile::tempdir().unwrap();
    let registry = crate::Registry::open(state.path()).unwrap();
    let mut config = registry.configuration();
    config.outcome_services.push(service(
        "external",
        OutcomeServiceKind::Knowledge,
        ServiceManagement::External,
        state.path(),
    ));
    let error = registry.save_settings(config).unwrap_err();
    assert_eq!(
        service_protocol::knowledge_error(&error).unwrap().code,
        contracts::KnowledgeErrorCode::ExternalServiceRemoved
    );
    assert_eq!(registry.configuration().revision, 0);
    assert!(registry.configuration().outcome_services.is_empty());
}

#[test]
fn owned_protocol_upgrade_is_backed_up_and_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path();
    let mut config = Configuration::defaults(state);
    config.outcome_services.push(service(
        "local",
        OutcomeServiceKind::Knowledge,
        ServiceManagement::LocalManaged,
        state,
    ));
    config.outcome_services[0].protocol_version = 1;
    let original = serde_json::to_value(&config).unwrap();
    let path = state.join("private/services.json");
    storage::write(&path, &config).unwrap();
    upgrade_local_protocol(&path, &mut config).unwrap();
    assert_eq!(
        config.outcome_services[0].protocol_version,
        service_api::OUTCOME_PROTOCOL_VERSION
    );
    assert_eq!(config.revision, 1);
    let backup = std::fs::read_dir(path.parent().unwrap())
        .unwrap()
        .map(|p| p.unwrap().path())
        .find(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("outcome-v1-before-")
        })
        .unwrap();
    assert_eq!(
        storage::read::<serde_json::Value>(&backup).unwrap(),
        original
    );
    upgrade_local_protocol(&path, &mut config).unwrap();
    assert_eq!(config.revision, 1);
}
