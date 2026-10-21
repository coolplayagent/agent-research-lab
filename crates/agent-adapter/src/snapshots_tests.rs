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
