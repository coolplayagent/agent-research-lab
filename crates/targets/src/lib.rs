pub mod desktop;

use anyhow::{Context, Result, ensure};
use config::Config;
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

pub fn prepare(c: &Config) -> Result<Value> {
    let _lock = storage::lock(&c.state_dir.join("desktop-prepare.lock"))?;
    desktop::prepare(&c.state_dir.join("tools/desktop"), Duration::from_secs(55))
}

pub fn status(c: &Config) -> Result<Value> {
    let dependencies = c.state_dir.join("tools/desktop/manifest.json");
    Ok(
        json!({"desktop":{"prepared":dependencies.is_file(),"dependencies_manifest":dependencies},
        "sandbox":{"provider":"local","execution_owner":"host","nested_user_namespace_required":false},
        "latest_verification": if c.state_dir.join("targets-latest.json").is_file() { Some(storage::read::<Value>(&c.state_dir.join("targets-latest.json"))?) } else { None },
        "readiness":"Preparation and help output alone do not prove a usable target; run targets verify."}),
    )
}

fn install_launcher(directory: &Path) -> Result<PathBuf> {
    fs::create_dir_all(directory)?;
    let bytes = fs::read(std::env::current_exe()?)?;
    let path = directory.join(format!(
        "target-controller-{}",
        &storage::digest(&bytes)[..16]
    ));
    if path.exists() {
        ensure!(
            fs::symlink_metadata(&path)?.is_file() && fs::read(&path)? == bytes,
            "target launcher changed"
        );
    } else {
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o500))?;
    }
    Ok(path)
}

fn desktop_arguments(c: &Config, output: &Path) -> Vec<String> {
    vec![
        "target-worker".into(),
        "--dependencies".into(),
        c.state_dir.join("tools/desktop").display().to_string(),
        "--output".into(),
        output.display().to_string(),
        "--computer-cli".into(),
        c.tools["computer-use-cli"].binary.display().to_string(),
    ]
}

/// Codex and its owned display share one task namespace. Desktop environment
/// variables refer only to the owned target; no host display socket is mounted.
/// API networking remains shared, so this is not network-level X11 isolation.
/// The worker lifetime bounds every target child.
pub fn wrap_desktop_agent(
    c: &Config,
    worktree: &Path,
    logs: &Path,
    write: bool,
    args: &[String],
) -> Result<(String, Vec<String>)> {
    wrap_desktop_backend(
        c,
        worktree,
        logs,
        write,
        &c.codex,
        args,
        &isolation::AgentAccess::codex(),
    )
}

pub fn wrap_desktop_backend(
    c: &Config,
    worktree: &Path,
    logs: &Path,
    write: bool,
    program: &str,
    args: &[String],
    access: &isolation::AgentAccess,
) -> Result<(String, Vec<String>)> {
    let launcher = install_launcher(logs)?;
    let worker_args = desktop_arguments(c, &logs.join("desktop"));
    isolation::wrap_agent_with_launcher_access(
        program,
        args,
        &c.state_dir,
        worktree,
        logs,
        write,
        (&launcher, &worker_args),
        access,
    )
}

/// Internal executable entry point, also used by the host verification command.
/// Returns the actual child exit status; never turns a failed agent into success.
pub fn worker(
    dependencies: &Path,
    output: &Path,
    computer_cli: &Path,
    verify: bool,
    command: &[String],
) -> Result<i32> {
    ensure!(
        verify == command.is_empty(),
        "select verification or an agent command"
    );
    let mut desktop = desktop::DesktopSession::start(output, dependencies)?;
    let inspection = desktop.inspect(computer_cli)?;
    storage::write(&output.join("inspection.json"), &inspection)?;
    if verify {
        let proof = desktop.verify(computer_cli, "lab-owned-real-input")?;
        storage::write(&output.join("verification.json"), &proof)?;
        println!("{}", serde_json::to_string(&proof)?);
        desktop.stop()?;
        return Ok(0);
    }
    let mut child = Command::new(&command[0]);
    child.args(&command[1..]);
    // Keep task-specific Codex/memory homes and provider authentication. Only
    // overlay the owned display and its native library/tool search paths.
    for (key, value) in desktop.environment() {
        if matches!(
            key.as_str(),
            "DISPLAY" | "XAUTHORITY" | "LD_LIBRARY_PATH" | "IMLIB2_LOADER_PATH"
        ) {
            child.env(key, value);
        }
    }
    child.env(
        "PATH",
        format!(
            "{}:{}",
            dependencies.join("usr/bin").display(),
            std::env::var("PATH").unwrap_or_default()
        ),
    );
    child
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("DBUS_SESSION_BUS_ADDRESS");
    child
        .env("LAB_DESKTOP_TARGET", output)
        .env("LAB_DESKTOP_WINDOW", desktop.window_title());
    let result = child.status().context("start agent in owned desktop")?;
    desktop.stop()?;
    Ok(result.code().unwrap_or(1))
}

pub fn verify(c: &Config, kind: &str) -> Result<Value> {
    ensure!(
        matches!(kind, "desktop" | "sandbox" | "all"),
        "target kind must be desktop, sandbox or all"
    );
    let id = format!(
        "targets-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_millis()
    );
    let logs = c.state_dir.join("runs").join(&id);
    let worktree = c.state_dir.join("worktrees").join(&id);
    fs::create_dir_all(&logs)?;
    fs::create_dir_all(&worktree)?;
    let mut result = json!({"id":id,"logs":logs,"verified_at":chrono::Utc::now().to_rfc3339()});
    if kind != "sandbox" {
        prepare(c)?;
        let executable = install_launcher(&c.state_dir.join("tools/target-controller"))?;
        let mut args = desktop_arguments(c, &logs.join("desktop"));
        args.push("--verify".into());
        let (program, args) = isolation::wrap_agent(
            executable.to_str().context("target executable path")?,
            &args,
            &c.state_dir,
            &worktree,
            &logs,
            true,
        )?;
        let out = process::capture(&program, &args, &worktree, Duration::from_secs(45))?;
        ensure!(
            out.status.success(),
            "real desktop verification failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        result["desktop"] = storage::read(&logs.join("desktop/verification.json"))?;
    }
    if kind != "desktop" {
        result["sandbox"] = sandbox_verify(c, &logs)?;
    }
    storage::write(&logs.join("targets.json"), &result)?;
    storage::write(&c.state_dir.join("targets-latest.json"), &result)?;
    Ok(result)
}

/// Bounded live model/tool compatibility probe through the production launcher.
/// This proves tool transport, not independent research quality or model benefit.
pub fn agent_check(c: &Config) -> Result<Value> {
    let backend_id = c
        .role_backends
        .get("review")
        .map(String::as_str)
        .unwrap_or("codex");
    let program = match c.agent_backends.get(backend_id) {
        Some(agent_backend::BackendSpec::Codex { program }) => program,
        Some(agent_backend::BackendSpec::JsonProcess { .. }) => anyhow::bail!(
            "desktop agent-check requires Codex command-event evidence; JSON bridge desktop compatibility is unverified; use an isolated task for backend-specific validation"
        ),
        None => &c.codex,
    };
    prepare(c)?;
    let id = format!(
        "target-agent-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_millis()
    );
    let logs = c.state_dir.join("runs").join(&id);
    let worktree = c.state_dir.join("worktrees").join(&id);
    fs::create_dir_all(&logs)?;
    fs::create_dir_all(&worktree)?;
    let args = agent_backend::codex_desktop_arguments(
        &c.models["review"],
        &logs.join("model-result.txt"),
        format!(
            "Verify the owned desktop using actual tools. The CLI is {}. Read its installed SKILL.md. Run list-windows and screenshot (global --allow-risk safe before subcommand) on the provided DISPLAY. Save screenshot to $LAB_DESKTOP_TARGET/model.png. Read $LAB_DESKTOP_TARGET/observed.json and report the exact owned window title. Do not change DISPLAY, XAUTHORITY, or write observation/session files. Use only this dedicated target. Report actual command failures honestly; do not claim unavailable tools worked. No Git, installs, external messages or other desktop actions.",
            c.tools["computer-use-cli"].binary.display()
        ),
    );
    storage::write(
        &logs.join("agent-permissions.json"),
        &agent_policy::description(),
    )?;
    let (program, args) = wrap_desktop_backend(
        c,
        &worktree,
        &logs,
        false,
        program,
        &args,
        &isolation::AgentAccess::codex(),
    )?;
    let mut worker = process::Process::spawn(
        &program,
        &args,
        &worktree,
        &logs.join("process"),
        Duration::from_secs(180),
        &[],
        None,
    )?;
    let status = loop {
        if let Some(status) = worker.poll()? {
            break status;
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    ensure!(
        status.success(),
        "live agent target check failed; inspect {}",
        logs.display()
    );
    let bytes = fs::read(&worker.stdout)?;
    ensure!(
        bytes.len() <= 16 * 1024 * 1024,
        "agent probe output too large"
    );
    let events: Vec<Value> = String::from_utf8(bytes)?
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    let commands: Vec<_> = events
        .iter()
        .filter_map(|event| {
            let item = &event["item"];
            (event["type"] == "item.completed"
                && item["type"] == "command_execution"
                && item["exit_code"] == 0)
                .then_some(item)
        })
        .collect();
    ensure!(
        !commands.is_empty(),
        "model exited without a successful tool command"
    );
    let screenshot = logs.join("desktop/model.png");
    let image =
        fs::read(&screenshot).context("model did not produce its requested real screenshot")?;
    ensure!(
        image.starts_with(b"\x89PNG\r\n\x1a\n"),
        "model screenshot is not PNG"
    );
    let observation: Value = storage::read(&logs.join("desktop/observed.json"))?;
    let proof = json!({"id":id,"model":c.models["review"],"agent_permissions":agent_policy::description(),"successful_tool_commands":commands.len(),"screenshot_sha256":storage::digest(&image),"owned_window":observation["title"],"logs":logs,"scope":"real model and tool transport smoke; model-writable artifacts are not independent evaluation"});
    storage::write(&logs.join("agent-check.json"), &proof)?;
    Ok(proof)
}

/// Host-side real CLI exercise: AppArmor may prohibit a nested user namespace,
/// so the controller creates the sandbox directly and supplies bounded evidence.
pub fn sandbox_verify(c: &Config, logs: &Path) -> Result<Value> {
    let root = logs.join("sandbox");
    fs::create_dir(&root).context("sandbox verification output must be new")?;
    let workspace = root.join("workspace");
    fs::create_dir(&workspace)?;
    let state = root.join("state");
    fs::create_dir(&state)?;
    let env = vec![
        ("REPO_SANDBOX_STATE_DIR".into(), state.display().to_string()),
        ("HOME".into(), root.display().to_string()),
        ("XDG_STATE_HOME".into(), state.display().to_string()),
    ];
    let cli = c.tools["repo-sandbox"]
        .binary
        .to_str()
        .context("sandbox binary path")?;
    let call = |args: &[&str]| -> Result<Value> {
        let out = process::capture_env(
            cli,
            &args.iter().map(|s| (*s).into()).collect::<Vec<_>>(),
            &workspace,
            Duration::from_secs(20),
            &env,
        )?;
        ensure!(
            out.status.success(),
            "repo-sandbox failed: {}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        Ok(serde_json::from_slice(&out.stdout)?)
    };
    let target = call(&[
        "target",
        "install",
        "--kind",
        "local",
        "--target",
        "lab-owned",
        "--json",
    ])?;
    ensure!(
        target["status"] == "ready"
            && target["provider"] == "bubblewrap"
            && target["persistent_namespace"] == false,
        "repo-sandbox did not prepare a real local provider"
    );
    let workspace_arg = workspace.to_str().context("sandbox workspace path")?;
    let up = call(&[
        "dev",
        "up",
        "--no-config",
        "--target",
        "lab-owned",
        "--host-tools",
        "--workspace",
        workspace_arg,
        "--session",
        "lab-check",
        "--json",
    ])?;
    let verification = (|| -> Result<Value> {
        require_session(&up, "ready")?;
        let executed = call(&[
            "dev",
            "exec",
            "--session",
            "lab-check",
            "--timeout-seconds",
            "10",
            "--json",
            "--",
            "/bin/sh",
            "-c",
            "printf real-local-sandbox > capability.txt; test -r /proc/self/status",
        ])?;
        require_execution(&executed)?;
        ensure!(
            fs::read_to_string(workspace.join("capability.txt"))? == "real-local-sandbox",
            "sandbox operation did not reach its owned workspace"
        );
        let observed = call(&["dev", "status", "--session", "lab-check", "--json"])?;
        require_session(&observed, "ready")?;
        Ok(
            json!({"target":target,"up":up,"execution":executed,"status":observed,"observed_file_sha256":storage::digest(b"real-local-sandbox")}),
        )
    })();
    // Always attempt to close a successfully created session, including failure.
    let down = call(&["dev", "down", "--session", "lab-check", "--json"]);
    let mut proof = verification?;
    let down = down?;
    require_session(&down, "stopped")?;
    proof["down"] = down;
    proof["verified"] = true.into();
    proof["runtime_sha256"] = storage::digest(&fs::read(&c.tools["repo-sandbox"].binary)?).into();
    storage::write(&root.join("verification.json"), &proof)?;
    Ok(proof)
}

fn require_execution(report: &Value) -> Result<()> {
    ensure!(
        report["provider"] == "bubblewrap"
            && report["persistent_namespace"] == false
            && report["status"] == "completed"
            && report["exit_code"] == 0
            && report["timed_out"] == false
            && report["network"] == "none"
            && report.get("error").is_none_or(Value::is_null),
        "repo-sandbox command did not complete successfully: {report}"
    );
    Ok(())
}

fn require_session(report: &Value, status: &str) -> Result<()> {
    ensure!(
        report["success"] == true
            && report["provider"] == "bubblewrap"
            && report["persistent_namespace"] == false
            && report["status"] == status
            && report["network"] == "none"
            && report["active_execution"].is_null(),
        "repo-sandbox session is not {status}: {report}"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_success_and_partial_side_effects_are_not_business_success() {
        let good = json!({"provider":"bubblewrap","persistent_namespace":false,"status":"completed","exit_code":0,"timed_out":false,"network":"none"});
        require_execution(&good).unwrap();
        for (key, value) in [
            ("exit_code", json!(7)),
            ("status", json!("timeout")),
            ("timed_out", json!(true)),
            ("error", json!("lost result")),
            ("network", json!("host")),
        ] {
            let mut bad = good.clone();
            bad[key] = value;
            assert!(require_execution(&bad).is_err(), "{key}");
        }
        let ready = json!({"success":true,"provider":"bubblewrap","persistent_namespace":false,"status":"ready","network":"none","active_execution":null});
        require_session(&ready, "ready").unwrap();
        assert!(require_session(&ready, "stopped").is_err());
        let mut active = ready;
        active["active_execution"] = json!({"pid":12});
        assert!(require_session(&active, "ready").is_err());
    }
}
