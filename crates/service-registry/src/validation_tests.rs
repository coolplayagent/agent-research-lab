use super::*;

#[test]
fn codex_login_failure_survives_service_hops_without_exposing_private_details() {
    for details in [
        "codex_login::auth::manager: status=401 error_code: Some(\"refresh_token_reused\")",
        "HTTP 401: {\"code\":\"token_expired\"}",
    ] {
        let failure: anyhow::Error = rpc::PrivateDiagnostic(details.into()).into();
        let sandbox = rpc::Reply::from_result(Err(failure));
        let executor = rpc::Reply::from_result(sandbox.decode::<serde_json::Value>());
        let error = executor.decode::<serde_json::Value>().unwrap_err();
        let code = execution_failure_code(CodingAgent::Codex, &error);
        assert_eq!(code, Code::AuthenticationRequired);
        assert_eq!(
            serde_json::to_string(&code).unwrap(),
            "\"authentication_required\""
        );
        assert!(!error.to_string().contains(details));
        assert_eq!(rpc::private_details(&error).unwrap(), details);
        assert_eq!(
            execution_failure_code(CodingAgent::Custom, &error),
            Code::VerificationFailed
        );
    }
}

#[test]
fn unknown_execution_errors_stay_failed_without_guessing_authentication() {
    for details in [
        "HTTP 401: unknown provider failure",
        "codex_login::auth::manager: network unavailable",
        "task could not write the challenge file",
    ] {
        let error: anyhow::Error = rpc::PrivateDiagnostic(details.into()).into();
        assert_eq!(
            execution_failure_code(CodingAgent::Codex, &error),
            Code::VerificationFailed
        );
    }
    let public_message = anyhow::anyhow!("401 token_expired");
    assert_eq!(
        execution_failure_code(CodingAgent::Codex, &public_message),
        Code::VerificationFailed
    );
}

#[test]
fn saved_launcher_arguments_and_model_are_preserved_without_reinterpreting_other_commands() {
    let mut adapter = Configuration::defaults(std::path::Path::new("/fixture"))
        .adapters
        .remove(0);
    adapter.command = vec![
        "/release/outcome-hub-agent".into(),
        "--agent".into(),
        "cc".into(),
        "--program".into(),
        "/runtime/node".into(),
        "--launcher-argument".into(),
        "--preserve-symlinks-main".into(),
        "--launcher-argument".into(),
        "/package/agent.js".into(),
        "--model".into(),
        "saved-model".into(),
    ];
    assert_eq!(
        saved_launch(&adapter),
        Some((
            PathBuf::from("/runtime/node"),
            vec![
                "--preserve-symlinks-main".into(),
                "/package/agent.js".into()
            ]
        ))
    );
    assert_eq!(
        saved_option(&adapter, "--model").as_deref(),
        Some("saved-model")
    );
    adapter
        .command
        .splice(7..7, ["--launcher-argument".into(), "--model".into()]);
    assert_eq!(
        saved_option(&adapter, "--model").as_deref(),
        Some("saved-model")
    );
    adapter.command[0] = "/custom/program".into();
    assert!(saved_launch(&adapter).is_none());
    assert!(saved_option(&adapter, "--model").is_none());
}

#[test]
fn identical_packaged_bridges_preserve_existing_binding_identity() {
    let root = tempfile::tempdir().unwrap();
    let prior = root.path().join("prior");
    let current = root.path().join("current");
    std::fs::create_dir_all(&prior).unwrap();
    std::fs::create_dir_all(&current).unwrap();
    let old = prior.join("outcome-hub-agent");
    let new = current.join("outcome-hub-agent");
    std::fs::write(&old, b"identical bridge fixture").unwrap();
    std::fs::write(&new, b"identical bridge fixture").unwrap();
    let mut adapter = Configuration::defaults(root.path()).adapters.remove(0);
    adapter.enabled = true;
    adapter.command = vec![
        old.to_string_lossy().into_owned(),
        "--program".into(),
        "/runtime/agent".into(),
    ];
    let before = serde_json::to_value(&adapter).unwrap();
    assert_eq!(bridge_path(&adapter, new.clone()).unwrap(), old);
    assert_eq!(serde_json::to_value(&adapter).unwrap(), before);
    adapter.enabled = false;
    assert_eq!(bridge_path(&adapter, new.clone()).unwrap(), new);
    adapter.enabled = true;
    std::fs::write(&new, b"changed bridge fixture").unwrap();
    assert_eq!(bridge_path(&adapter, new.clone()).unwrap(), new);
    std::fs::remove_file(&old).unwrap();
    assert_eq!(bridge_path(&adapter, new.clone()).unwrap(), new);
    std::fs::remove_file(&new).unwrap();
    assert!(bridge_path(&adapter, new).is_err());
}
