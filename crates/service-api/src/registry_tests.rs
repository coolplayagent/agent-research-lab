use super::*;

#[test]
fn default_service_state_matches_available_transport() {
    let config = Configuration::defaults(Path::new("/tmp/outcome-hub"));
    assert!(
        config
            .services
            .iter()
            .all(|service| service.enabled == cfg!(unix))
    );
    assert!(config.storage_id.is_none());
}
