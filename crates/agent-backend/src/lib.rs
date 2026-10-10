//! Versioned local execution adapters. Capabilities are host configuration,
//! not compatibility evidence or permission to bypass the controller.
use anyhow::{Context, Result, ensure};
use config::Config;
use isolation::AgentAccess;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};
use task::Task;

pub use agent_policy::validate_environment_name;
pub use config::{BackendSpec, Binding, Capabilities, validate_backends as validate_config};

pub fn access_spec(spec: &BackendSpec) -> AgentAccess {
    match spec {
        BackendSpec::Codex { .. } => AgentAccess::codex(),
        BackendSpec::JsonProcess { env_allowlist, .. } => AgentAccess {
            codex_credentials: false,
            env_allowlist: env_allowlist.clone(),
            read_only_views: vec![],
        },
    }
}

pub fn check_capabilities(spec: &BackendSpec, task: &Task) -> Result<()> {
    let caps = spec.capabilities();
    ensure!(
        caps.structured_result && caps.read_workspace,
        "backend lacks structured result or workspace-read capability"
    );
    ensure!(
        !task.write || caps.write_workspace,
        "backend lacks workspace-write capability"
    );
    ensure!(
        (task.required_tools.is_empty() && task.communication.is_none()) || caps.tool_execution,
        "backend lacks tool-execution capability"
    );
    ensure!(
        !task.required_tools.iter().any(|t| t == "computer-use-cli") || caps.desktop,
        "backend lacks owned-desktop capability"
    );
    Ok(())
}

pub fn freeze(c: &Config, task: &Task) -> Result<Option<Binding>> {
    freeze_selected(c, task, None)
}

pub fn validate_preference(c: &Config, preference: &task::ExecutionPreference) -> Result<()> {
    if let Some(id) = &preference.backend {
        ensure!(
            id == "codex" || c.agent_backends.contains_key(id),
            "unknown execution backend"
        );
    }
    if let Some(model) = &preference.model {
        ensure!(
            c.models
                .values()
                .chain(c.backend_models.values().flatten())
                .any(|configured| configured == model),
            "model is not configured by the host"
        );
        if let Some(backend) = &preference.backend {
            validate_model(c, backend, model)?;
        }
    }
    Ok(())
}

pub fn validate_model(c: &Config, backend: &str, model: &str) -> Result<()> {
    let allowed = c.backend_models.get(backend).map_or_else(
        || c.models.values().any(|m| m == model),
        |models| models.iter().any(|m| m == model),
    );
    ensure!(
        allowed,
        "model is not configured for execution backend {backend}"
    );
    Ok(())
}

pub fn freeze_selected(c: &Config, task: &Task, selected: Option<&str>) -> Result<Option<Binding>> {
    validate_config(c)?;
    let id = selected
        .or_else(|| c.role_backends.get(&task.role).map(String::as_str))
        .unwrap_or("codex");
    if id == "codex" {
        return Ok(None);
    }
    let spec = c.agent_backends.get(id).context("unknown backend")?;
    check_capabilities(spec, task)?;
    let executable = isolation::executable(spec.program())?;
    let executable_sha256 = storage::digest(&read_regular(&executable, 256 * 1024 * 1024)?);
    Ok(Some(Binding {
        id: id.into(),
        spec: spec.clone(),
        executable,
        executable_sha256,
    }))
}

/// Verify against the frozen profile, never the person's mutable current preference.
pub fn verify_job(c: &Config, job: &task::Job) -> Result<()> {
    let preference = job.persona.as_ref().and_then(|p| p.execution.as_ref());
    if let Some(preference) = preference {
        validate_preference(c, preference)?;
    }
    let model = preference
        .and_then(|p| p.model.as_ref())
        .or_else(|| c.models.get(&job.task.role));
    ensure!(model == Some(&job.model), "frozen model selection changed");
    validate_model(
        c,
        job.backend.as_ref().map_or("codex", |b| b.id.as_str()),
        &job.model,
    )?;
    ensure!(
        freeze_selected(c, &job.task, preference.and_then(|p| p.backend.as_deref()))?
            == job.backend,
        "frozen backend configuration, capability or executable changed"
    );
    Ok(())
}

/// Browser-safe inventory: no programs, arguments, credential names or host paths.
pub fn public_inventory(c: &Config) -> Value {
    let builtin = BackendSpec::Codex {
        program: c.codex.clone(),
    };
    let executors: Vec<_> = std::iter::once(("codex", &builtin))
        .chain(c.agent_backends.iter().map(|(id, spec)| (id.as_str(), spec)))
        .map(|(id, spec)| json!({"id":id,"kind":match spec {BackendSpec::Codex{..}=>"codex",BackendSpec::JsonProcess{..}=>"json_process"},"models":c.backend_models.get(id).cloned().unwrap_or_else(|| c.models.values().cloned().collect::<std::collections::BTreeSet<_>>().into_iter().collect()),"capabilities":spec.capabilities(),"verification":"configured_only"})).collect();
    json!({"backends":executors,"models":c.models.values().chain(c.backend_models.values().flatten()).collect::<std::collections::BTreeSet<_>>(),"role_backends":c.role_backends})
}

pub fn verify(c: &Config, task: &Task, binding: Option<&Binding>) -> Result<()> {
    ensure!(
        freeze(c, task)?.as_ref() == binding,
        "frozen backend configuration, capability or executable changed"
    );
    Ok(())
}

pub fn access(binding: Option<&Binding>) -> AgentAccess {
    binding.map_or_else(AgentAccess::codex, |b| access_spec(&b.spec))
}

pub fn codex_arguments(model: &str, schema: &Path, result: &Path) -> Vec<String> {
    let mut args = vec![
        "exec".into(),
        "--json".into(),
        "--ephemeral".into(),
        "-m".into(),
        model.into(),
        "-c".into(),
        "model_reasoning_effort=\"medium\"".into(),
        "--output-schema".into(),
        schema.display().to_string(),
        "-o".into(),
        result.display().to_string(),
    ];
    args.extend(agent_policy::arguments());
    args.push("-".into());
    args
}

pub fn codex_probe_arguments(model: &str) -> Vec<String> {
    let mut args = vec![
        "exec".into(),
        "--ephemeral".into(),
        "--json".into(),
        "-m".into(),
        model.into(),
    ];
    args.extend(agent_policy::arguments());
    args.push("Reply with exactly OK. Do not use tools.".into());
    args
}

pub fn codex_desktop_arguments(model: &str, result: &Path, prompt: String) -> Vec<String> {
    let mut args: Vec<_> = [
        "exec",
        "--json",
        "--ephemeral",
        "--skip-git-repo-check",
        "-m",
        model,
        "-o",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    args.push(result.display().to_string());
    args.extend(agent_policy::arguments());
    args.push(prompt);
    args
}

pub struct Invocation {
    pub program: String,
    pub args: Vec<String>,
    pub stdin: PathBuf,
    /// Persist this in host-only launch authority before spawn; never recompute
    /// from a request file after a worker has been allowed to write its logs.
    pub request_sha256: Option<String>,
    pub access: AgentAccess,
}

pub struct RequestContext<'a> {
    pub request_id: &'a str,
    pub model: &'a str,
    pub prompt_path: &'a Path,
    pub schema_path: &'a Path,
    pub worktree: &'a Path,
    pub logs: &'a Path,
    pub bindings: Value,
    pub write: bool,
    pub required_tools: &'a [String],
    pub timeout_seconds: u64,
}

pub fn prepare(
    codex: &str,
    binding: Option<&Binding>,
    context: RequestContext<'_>,
) -> Result<Invocation> {
    let program = binding.map_or(codex, |b| b.executable.to_str().unwrap_or(""));
    if let Some(Binding {
        spec: BackendSpec::JsonProcess { args, .. },
        ..
    }) = binding
    {
        let request = json!({
            "schema_version":1,"request_id":context.request_id,"model":context.model,
            "prompt":String::from_utf8(read_regular(context.prompt_path, 2*1024*1024)?)?,
            "response_schema":serde_json::from_slice::<Value>(&read_regular(context.schema_path, 1024*1024)?)?,
            "worktree":context.worktree,"result_path":context.logs.join("bridge-result.json"),
            "bindings":context.bindings,"backend":binding,
            "permissions":{"worktree_write":context.write,"required_tools":context.required_tools,"external_writes":false,"nested_agents":false,"authority":"host controller; backend declarations do not grant authority"},
            "timeout_seconds":context.timeout_seconds
        });
        let mut bytes = serde_json::to_vec(&request)?;
        bytes.push(b'\n');
        ensure!(
            bytes.len() <= 3 * 1024 * 1024,
            "bridge request exceeds 3 MiB"
        );
        let digest = storage::digest(&bytes);
        let stdin = context.logs.join("bridge-request.json");
        ensure!(
            !stdin.exists(),
            "bridge request already exists for this attempt"
        );
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&stdin)?
            .write_all(&bytes)?;
        Ok(Invocation {
            program: program.into(),
            args: args.clone(),
            stdin,
            request_sha256: Some(digest),
            access: access(binding),
        })
    } else {
        Ok(Invocation {
            program: program.into(),
            args: codex_arguments(
                context.model,
                context.schema_path,
                &context.logs.join("result.json"),
            ),
            stdin: context.prompt_path.into(),
            request_sha256: None,
            access: access(binding),
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BridgeResult {
    schema_version: u32,
    request_sha256: String,
    report: Value,
}

pub fn result(binding: Option<&Binding>, logs: &Path, digest: Option<&str>) -> Result<Value> {
    if binding.is_some_and(|b| matches!(b.spec, BackendSpec::JsonProcess { .. })) {
        let envelope: BridgeResult = serde_json::from_slice(&read_regular(
            &logs.join("bridge-result.json"),
            1024 * 1024,
        )?)?;
        ensure!(
            envelope.schema_version == 1
                && Some(envelope.request_sha256.as_str()) == digest
                && digest.is_some(),
            "bridge result does not bind the host-frozen request"
        );
        Ok(envelope.report)
    } else {
        ensure!(
            digest.is_none(),
            "unexpected bridge request for Codex backend"
        );
        Ok(serde_json::from_slice(&read_regular(
            &logs.join("result.json"),
            1024 * 1024,
        )?)?)
    }
}

pub fn read_regular(path: &Path, maximum: u64) -> Result<Vec<u8>> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file() && metadata.len() <= maximum,
        "backend artifact is not a bounded regular file"
    );
    let mut bytes = Vec::new();
    file.take(maximum + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= maximum,
        "backend artifact grew beyond bound"
    );
    Ok(bytes)
}

/// A transport/contract probe, not evidence that a third-party provider or model
/// performs useful research. Generic probes run under the same isolation policy.
pub fn probe(c: &Config, role: &str, model: &str) -> Result<Value> {
    use std::time::Duration;
    let task: Task = serde_json::from_value(json!({"id":"backend-probe","role":role,
        "repository":"superpod","prompt":"Return the requested probe result without tools."}))?;
    let binding = freeze(c, &task)?;
    if !binding
        .as_ref()
        .is_some_and(|b| matches!(b.spec, BackendSpec::JsonProcess { .. }))
    {
        let program = binding
            .as_ref()
            .map_or(c.codex.as_str(), |b| b.executable.to_str().unwrap_or(""));
        let output = process::capture(
            program,
            &codex_probe_arguments(model),
            &c.superpod,
            Duration::from_secs(90),
        )?;
        return Ok(
            json!({"model":model,"backend":binding.as_ref().map_or("codex", |b| b.id.as_str()),"available":output.status.success(),"probe":"codex_model_response"}),
        );
    }
    let id = format!(
        "backend-probe-{}-{}",
        std::process::id(),
        chrono::Utc::now()
            .timestamp_nanos_opt()
            .context("probe timestamp")?
    );
    let logs = c.state_dir.join("runs").join(&id);
    let worktree = c.state_dir.join("worktrees").join(&id);
    fs::create_dir_all(&logs)?;
    fs::create_dir_all(&worktree)?;
    let prompt = logs.join("prompt.txt");
    let schema = logs.join("result-schema.json");
    fs::write(
        &prompt,
        "Return exactly {\"ok\":true} as the report. Do not use tools. This checks only the local JSON bridge contract.",
    )?;
    storage::write(
        &schema,
        &json!({"type":"object","properties":{"ok":{"type":"boolean","const":true}},"required":["ok"],"additionalProperties":false}),
    )?;
    let invocation = prepare(
        &c.codex,
        binding.as_ref(),
        RequestContext {
            request_id: &id,
            model,
            prompt_path: &prompt,
            schema_path: &schema,
            worktree: &worktree,
            logs: &logs,
            bindings: json!({"probe":true}),
            write: false,
            required_tools: &[],
            timeout_seconds: 90,
        },
    )?;
    verify(c, &task, binding.as_ref())?;
    let (program, args) = isolation::wrap_agent_with_access(
        &invocation.program,
        &invocation.args,
        &c.state_dir,
        &worktree,
        &logs,
        false,
        &invocation.access,
    )?;
    let mut process = process::Process::spawn(
        &program,
        &args,
        &worktree,
        &logs.join("process"),
        Duration::from_secs(90),
        &[],
        Some(&invocation.stdin),
    )?;
    loop {
        if let Some(status) = process.poll()? {
            ensure!(status.success(), "bridge probe exited unsuccessfully");
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    ensure!(
        result(
            binding.as_ref(),
            &logs,
            invocation.request_sha256.as_deref()
        )? == json!({"ok":true}),
        "bridge probe report does not satisfy the probe schema"
    );
    Ok(
        json!({"model":model,"backend":binding.as_ref().map(|b| &b.id),"available":true,"probe":"isolated_json_contract","provider_compatibility":"unverified","evidence":logs}),
    )
}

pub fn inventory(c: &Config) -> Value {
    Value::Array(c.agent_backends.iter().map(|(id,spec)| json!({
        "id":id,"kind":match spec {BackendSpec::Codex {..}=>"codex", BackendSpec::JsonProcess {..}=>"json_process"},
        "program":spec.program(),"capabilities":spec.capabilities(),"compatibility":"unverified until tested; capabilities are declarations"
    })).collect())
}

pub fn permission_description(binding: Option<&Binding>) -> Value {
    if binding.is_some_and(|b| matches!(b.spec, BackendSpec::JsonProcess { .. })) {
        json!({"backend":"json_process","authority":"host controller","filesystem":"task worktree access and own logs only writable as permitted; host HOME hidden","credentials":"explicit environment names only; no Codex credentials","capabilities":"declared, not verified","external_writes":false,"nested_agents":false})
    } else {
        agent_policy::description()
    }
}

#[cfg(test)]
mod tests {
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
}
