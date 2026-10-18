use super::*;
use std::os::unix::fs::{PermissionsExt, symlink};
fn task() -> Task {
    serde_json::from_value(
        json!({"id":"fixture","role":"research","repository":"superpod","prompt":"fixture"}),
    )
    .unwrap()
}
fn spec(path: &Path) -> BackendSpec {
    BackendSpec::JsonProcess {
        program: path.into(),
        args: vec![],
        env_allowlist: vec![],
        capabilities: Capabilities {
            structured_result: true,
            read_workspace: true,
            ..Capabilities::default()
        },
    }
}
fn binding(path: &Path) -> Binding {
    Binding {
        id: "fixture".into(),
        spec: spec(path),
        executable: path.into(),
        executable_sha256: "a".repeat(64),
    }
}
#[test]
fn missing_capabilities_and_host_authority_environment_are_rejected() {
    let mut task = task();
    let spec = spec(Path::new("/bin/true"));
    assert!(check_capabilities(&spec, &task).is_ok());
    task.write = true;
    assert!(check_capabilities(&spec, &task).is_err());
    task.write = false;
    task.required_tools = vec!["repo-sandbox".into()];
    assert!(check_capabilities(&spec, &task).is_err());
    for key in [
        "GH_TOKEN",
        "GIT_CONFIG_GLOBAL",
        "SSH_AUTH_SOCK",
        "LD_PRELOAD",
        "HOME",
        "CODEX_HOME",
        "NODE_OPTIONS",
    ] {
        assert!(validate_environment_name(key).is_err(), "{key}");
    }
    assert!(validate_environment_name("ANTHROPIC_API_KEY").is_ok());
    assert!(validate_environment_name("OPENAI_API_KEY").is_ok());
}
#[test]
fn result_requires_host_frozen_digest_and_bounded_regular_file() {
    let temp = tempfile::tempdir().unwrap();
    let logs = temp.path();
    let path = logs.join("bridge-result.json");
    let binding = binding(Path::new("/bin/true"));
    let digest = storage::digest(b"original request\n");
    storage::write(
        &path,
        &json!({"schema_version":1,"request_sha256":digest,"report":{"ok":true}}),
    )
    .unwrap();
    assert_eq!(
        result(Some(&binding), logs, Some(&digest)).unwrap(),
        json!({"ok":true})
    );
    fs::write(logs.join("bridge-request.json"), b"worker modified request").unwrap();
    storage::write(&path,&json!({"schema_version":1,"request_sha256":storage::digest(b"worker modified request"),"report":{"ok":true}})).unwrap();
    assert!(result(Some(&binding), logs, Some(&digest)).is_err());
    fs::write(&path, b"not JSON").unwrap();
    assert!(result(Some(&binding), logs, Some(&digest)).is_err());
    fs::write(&path, vec![b' '; 1024 * 1024 + 1]).unwrap();
    assert!(result(Some(&binding), logs, Some(&digest)).is_err());
    fs::remove_file(&path).unwrap();
    symlink(logs.join("bridge-request.json"), &path).unwrap();
    assert!(result(Some(&binding), logs, Some(&digest)).is_err());
    fs::remove_file(&path).unwrap();
    let name = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    let started = std::time::Instant::now();
    assert!(result(Some(&binding), logs, Some(&digest)).is_err());
    assert!(started.elapsed().as_secs() < 1);
}
#[test]
fn legacy_config_bytes_and_explicit_binding_detect_executable_drift() {
    let legacy = r#"{"schema_version":1,"workspace":"/tmp/workspace","state_dir":"/tmp/state","superpod":"/tmp/superpod","codex":"codex","workflow":"/bin/true","daily_seconds":100,"max_agents":1,"task_timeout_seconds":10,"models":{"research":"fixture"},"tools":{},"require_latest":false,"skills_manifest":null}"#;
    let mut c: Config = serde_json::from_str(legacy).unwrap();
    assert_eq!(serde_json::to_string(&c).unwrap(), legacy);
    assert!(freeze(&c, &task()).unwrap().is_none());
    let temp = tempfile::tempdir().unwrap();
    let executable = temp.path().join("bridge");
    fs::write(&executable, b"#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
    c.agent_backends.insert("fixture".into(), spec(&executable));
    c.role_backends.insert("research".into(), "fixture".into());
    let frozen = freeze(&c, &task()).unwrap();
    verify(&c, &task(), frozen.as_ref()).unwrap();
    fs::write(&executable, b"#!/bin/sh\nexit 1\n").unwrap();
    assert!(verify(&c, &task(), frozen.as_ref()).is_err());
    c.role_backends.clear();
    assert!(verify(&c, &task(), frozen.as_ref()).is_err());
}
#[test]
fn codex_adapter_preserves_argument_and_permission_contract() {
    let args = codex_arguments("model", Path::new("/schema"), Path::new("/result"));
    assert_eq!(
        &args[..11],
        &[
            "exec",
            "--json",
            "--ephemeral",
            "-m",
            "model",
            "-c",
            "model_reasoning_effort=\"medium\"",
            "--output-schema",
            "/schema",
            "-o",
            "/result"
        ]
    );
    assert_eq!(&args[11..args.len() - 1], agent_policy::arguments());
    assert_eq!(args.last().unwrap(), "-");
}
