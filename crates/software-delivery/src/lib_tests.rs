use super::*;
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
#[test]
fn rejected_admission_does_not_leave_a_phantom_pending_activation() {
    let mut round = collaboration_model::EvolutionRound {
        number: 1,
        state: RoundState::Accepted,
        goal_id: "goal".into(),
        goal_attempts: BTreeMap::new(),
        baseline_version_id: "baseline".into(),
        started_ms: 1,
        finished_ms: None,
        feedback: String::new(),
        candidate: None,
        evaluation: None,
        activation: None,
    };
    assert_eq!(confirmed_round_claim(&round, "claim", false).unwrap(), None);
    assert!(confirmed_round_claim(&round, "claim", true).is_err());
    round.activation = Some(collaboration_model::CandidateActivation {
        state: ActivationState::Pending,
        reference: "claim".into(),
        evidence: json!({"activation_claim_id":"claim"}),
    });
    assert_eq!(
        confirmed_round_claim(&round, "claim", false).unwrap(),
        Some("claim".into())
    );
    assert!(confirmed_round_claim(&round, "different-claim", false).is_err());
}
#[test]
fn activation_rejects_empty_failed_or_stale_full_gate() {
    let commit = "a".repeat(40);
    let valid = json!({"profile":"full","scope":"delivery","gate":{"complete":true,"decision":"pass"},"selection":{"empty_delivery":false},"snapshot":{"mode":"diff","head":commit},"plan":{"pending_delivery_checks":[]}});
    validate_gate(&valid, &commit).unwrap();
    for (pointer, value) in [
        ("/selection/empty_delivery", json!(true)),
        ("/snapshot/head", json!("b".repeat(40))),
        ("/gate/complete", json!(false)),
        ("/plan/pending_delivery_checks", json!(["test"])),
    ] {
        let mut bad = valid.clone();
        *bad.pointer_mut(pointer).unwrap() = value;
        assert!(validate_gate(&bad, &commit).is_err());
    }
}
#[test]
fn artifact_paths_and_service_units_cannot_escape_host_configuration() {
    for value in ["../binary", "/bin/tool", "assets/../tool", ""] {
        assert!(relative(Path::new(value)).is_err());
    }
    relative(Path::new("assets/linux/tool")).unwrap();
    for value in ["../service", "service\nExecStart=evil", "service;evil", ""] {
        assert!(unit(value).is_err());
    }
    unit("agent-research-lab.service").unwrap();
}
#[test]
fn platform_binary_replacement_preserves_endpoints_and_environment() {
    let previous = "[Service]\nEnvironmentFile=/private/service.env\nExecStart=/old/ai-im-storage --state-dir /durable/store --socket /private/storage.sock --max-seconds 43200\nRestart=always\n";
    let changed = replace_executable(previous, Path::new("/new/ai-im-storage")).unwrap();
    assert!(changed.contains("EnvironmentFile=/private/service.env"));
    assert!(changed.contains("ExecStart=/new/ai-im-storage --state-dir /durable/store --socket /private/storage.sock --max-seconds 43200"));
    assert!(replace_executable("ExecStart=/one\nExecStart=/two", Path::new("/new")).is_err());
}
#[test]
fn health_checks_use_actual_http_status_and_exact_host() {
    for (status, succeeds) in [
        ("200 OK", true),
        ("401 Unauthorized", false),
        ("503 Unavailable", false),
    ] {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = Vec::new();
            while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
                let mut buffer = [0u8; 256];
                let count = stream.read(&mut buffer).unwrap();
                assert!(count > 0 && request.len() + count <= 1024);
                request.extend_from_slice(&buffer[..count]);
            }
            assert!(String::from_utf8_lossy(&request).contains(&format!("Host: {address}\r\n")));
            write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            )
            .unwrap();
        });
        assert_eq!(http_health(&format!("http://{address}/")).is_ok(), succeeds);
        server.join().unwrap();
    }
}
