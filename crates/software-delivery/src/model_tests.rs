use super::*;
use serde_json::json;

#[test]
fn merged_snapshot_budget_preserves_legacy_policy_and_full_gate_arguments() {
    let legacy = r#"{"schema_version":1,"service_config":"/config.toml","main_unit":"main.service","health_url":"http://127.0.0.1:8090/","repositories":{"owner/repo":{"checkout":"/repo","build_commands":[["cargo","build","--release"]],"package_directory":"dist/skill","binary_relative_path":"assets/linux/tool","probe":["--help"],"platform_units":[]}},"storage_state_dir":null,"storage_socket":"/storage.sock"}"#;
    let policy: Policy = serde_json::from_str(legacy).unwrap();
    let bytes = serde_json::to_vec(&policy).unwrap();
    assert_eq!(bytes, legacy.as_bytes());
    assert_eq!(
        storage::digest(&bytes),
        "51a4a95ee6a1bd02493a3d442daa4bceb001e87ce4a70b9a6b23319d613012b2"
    );
    let expected = [
        "qualitygate",
        "check",
        "--root",
        "/merged",
        "--diff",
        "merged^..merged",
        "--profile",
        "full",
        "--format",
        "json",
    ]
    .map(str::to_owned)
    .to_vec();
    assert_eq!(
        policy.repositories["owner/repo"].qualitygate_command(
            "qualitygate".into(),
            Path::new("/merged"),
            "merged",
        ),
        expected
    );
    for budget in [1, 5, 8] {
        let mut value: Value = serde_json::from_str(legacy).unwrap();
        value["repositories"]["owner/repo"]["qualitygate_snapshot_max_file_mib"] = json!(budget);
        let configured: Policy = serde_json::from_value(value).unwrap();
        let args = configured.repositories["owner/repo"].qualitygate_command(
            "qualitygate".into(),
            Path::new("/merged"),
            "merged",
        );
        assert_eq!(args[..expected.len()], expected);
        assert_eq!(
            args[expected.len()..],
            ["--snapshot-max-file-mib".to_owned(), budget.to_string()]
        );
        assert_ne!(
            storage::digest(&serde_json::to_vec(&configured).unwrap()),
            storage::digest(&bytes)
        );
        assert_eq!(
            serde_json::to_value(&configured).unwrap()["repositories"]["owner/repo"]["qualitygate_snapshot_max_file_mib"],
            budget
        );
    }
    for invalid in [
        json!(0),
        json!(9),
        json!(-1),
        json!(256),
        json!(1.5),
        json!("5"),
        Value::Null,
    ] {
        let mut value: Value = serde_json::from_str(legacy).unwrap();
        value["repositories"]["owner/repo"]["qualitygate_snapshot_max_file_mib"] = invalid;
        assert!(serde_json::from_value::<Policy>(value).is_err());
    }
}
