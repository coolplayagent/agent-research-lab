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
