use super::*;

#[test]
fn npm_runtime_preserves_hoisted_dependency_layout() {
    let root = tempfile::tempdir().unwrap();
    let modules = root.path().join("node_modules");
    let package = modules.join("@vendor/agent");
    fs::create_dir_all(package.join("dist")).unwrap();
    fs::create_dir_all(modules.join("dependency")).unwrap();
    fs::write(package.join("package.json"), "{}").unwrap();
    fs::write(package.join("dist/cli.js"), "require('dependency')").unwrap();
    fs::write(modules.join("dependency/index.js"), "module.exports = {}").unwrap();
    let executable = root.path().join("node.exe");
    let resolved = ResolvedAgent {
        agent: CodingAgent::Pi,
        executable: executable.clone(),
        arguments: vec![package.join("dist/cli.js").to_string_lossy().into_owned()],
        version: "fixture".into(),
        digest: "fixture".into(),
    };
    assert_eq!(runtime_paths(&resolved).unwrap(), vec![executable, modules]);
}
#[test]
fn snapshot_rejects_directories_and_oversized_configuration() {
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("copy");
    assert!(copy_file(temp.path(), &target).is_err());
    let source = temp.path().join("config");
    fs::write(&source, vec![0; 1024 * 1024 + 1]).unwrap();
    assert!(copy_file(&source, &target).is_err());
    fs::write(&source, b"private").unwrap();
    copy_file(&source, &target).unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"private");
}

#[test]
fn codex_runtime_mounts_its_native_tool_host() {
    let root = tempfile::tempdir().unwrap();
    let executable = root.path().join("codex");
    let host = root.path().join("codex-code-mode-host");
    fs::write(&executable, b"runtime").unwrap();
    fs::write(&host, b"host").unwrap();
    let resolved = ResolvedAgent {
        agent: CodingAgent::Codex,
        executable: executable.clone(),
        arguments: vec![],
        version: "fixture".into(),
        digest: "fixture".into(),
    };
    assert_eq!(runtime_paths(&resolved).unwrap(), vec![executable, host]);
}
#[test]
fn network_routing_is_forwarded_by_name_only() {
    for agent in [
        CodingAgent::Codex,
        CodingAgent::ClaudeCode,
        CodingAgent::Pi,
        CodingAgent::DeepseekHarness,
    ] {
        let names = environment_names(agent);
        for name in [
            "HTTPS_PROXY",
            "HTTP_PROXY",
            "ALL_PROXY",
            "NO_PROXY",
            "https_proxy",
            "http_proxy",
            "all_proxy",
            "no_proxy",
        ] {
            assert_eq!(
                names.iter().any(|n| n == name),
                std::env::var_os(name).is_some()
            );
        }
        assert!(names.iter().all(|name| !name.contains("://")));
    }
}

#[test]
fn codex_uses_current_login_without_copying_user_configuration_or_memories() {
    let root = tempfile::tempdir().unwrap();
    let service = root.path().join("service");
    let login = root.path().join("login");
    fs::create_dir_all(&service).unwrap();
    fs::create_dir_all(&login).unwrap();
    fs::write(service.join("config.toml"), "model = 'service-model'").unwrap();
    fs::write(service.join("auth.json"), "obsolete").unwrap();
    fs::write(login.join("config.toml"), "model = 'desktop-model'").unwrap();
    fs::write(login.join("sessions.sqlite"), "private history").unwrap();
    let auth = login.join("auth.json");
    fs::write(&auth, "current-login").unwrap();
    let first = root.path().join("first");
    codex_configuration(&service, Some(&auth), &first).unwrap();
    assert_eq!(
        fs::read_to_string(first.join("auth.json")).unwrap(),
        "current-login"
    );
    assert_eq!(
        fs::read_to_string(first.join("config.toml")).unwrap(),
        "model = 'service-model'"
    );
    assert_eq!(fs::read_dir(&first).unwrap().count(), 2);
    fs::write(first.join("auth.json"), "task-local-change").unwrap();
    assert_eq!(fs::read_to_string(&auth).unwrap(), "current-login");
    fs::write(&auth, "renewed-login").unwrap();
    let second = root.path().join("second");
    codex_configuration(&service, Some(&auth), &second).unwrap();
    assert_eq!(
        fs::read_to_string(second.join("auth.json")).unwrap(),
        "renewed-login"
    );
    assert_eq!(
        fs::read_to_string(service.join("auth.json")).unwrap(),
        "obsolete"
    );
}

#[test]
fn explicit_codex_login_never_falls_back_to_a_stale_copy() {
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("target");
    fs::write(root.path().join("auth.json"), "default-login").unwrap();
    codex_configuration(root.path(), None, &target).unwrap();
    assert_eq!(
        fs::read_to_string(target.join("auth.json")).unwrap(),
        "default-login"
    );
    let missing = root.path().join("missing.json");
    assert!(codex_configuration(root.path(), Some(&missing), &target).is_err());
    assert!(codex_configuration(root.path(), Some(Path::new("auth.json")), &target).is_err());
    assert!(codex_configuration(root.path(), Some(root.path()), &target).is_err());
    fs::write(&missing, vec![0; 1024 * 1024 + 1]).unwrap();
    assert!(codex_configuration(root.path(), Some(&missing), &target).is_err());
    #[cfg(unix)]
    {
        let link = root.path().join("link.json");
        std::os::unix::fs::symlink(root.path().join("auth.json"), &link).unwrap();
        assert!(codex_configuration(root.path(), Some(&link), &target).is_err());
    }
}
