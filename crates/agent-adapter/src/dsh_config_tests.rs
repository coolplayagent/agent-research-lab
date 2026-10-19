use super::*;
#[test]
fn existing_route_is_preserved_and_ambiguous_routes_require_override() {
    assert_eq!(route("- config:\n    agents:\n      - id: main\n        provider: local-route\n        model: selected-model\n").unwrap(), "local-route/selected-model");
    assert!(route("agents: [{provider: a, model: one}, {provider: b, model: two}]").is_err());
    assert!(route("defaultModel: replacement").is_err());
    assert!(route("agents: [{provider: !js 'secret()', model: selected}]").is_err());
}
