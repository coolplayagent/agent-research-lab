//! Linux mount and PID isolation for task agents. Host files are read-only;
//! controller state is hidden, except the task's own worktree/logs and the
//! read-only tool installation. Network access remains available to model APIs.

use anyhow::{Context, Result, ensure};
use native_effects::fs;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path, PathBuf};

/// Build a bubblewrap invocation. Missing isolation support is an error at
/// launch, never a reason to run the agent directly. All filesystem arguments
/// must be existing canonical directories without symlink components.
///
/// `worktree` must be below state_dir/worktrees, and `log_dir` below
/// state_dir/runs. Holdouts and host gate receipts belong below
/// state_dir/private and are never mounted in the child.
pub fn wrap_agent(
    program: &str,
    args: &[String],
    state_dir: &Path,
    worktree: &Path,
    log_dir: &Path,
    write: bool,
) -> Result<(String, Vec<String>)> {
    wrap_agent_with_access(
        program,
        args,
        state_dir,
        worktree,
        log_dir,
        write,
        &AgentAccess::codex(),
    )
}

/// Host-selected access, never supplied by a backend result. Views must be exact
/// public cohort directories or this run's inbox; controller authority and other
/// runs' inboxes are not mountable here.
#[derive(Debug, Clone)]
pub struct AgentAccess {
    pub codex_credentials: bool,
    pub env_allowlist: Vec<String>,
    pub read_only_views: Vec<PathBuf>,
}
impl AgentAccess {
    pub fn codex() -> Self {
        Self {
            codex_credentials: true,
            env_allowlist: vec![],
            read_only_views: vec![],
        }
    }
}

pub fn wrap_agent_with_access(
    program: &str,
    args: &[String],
    state_dir: &Path,
    worktree: &Path,
    log_dir: &Path,
    write: bool,
    access: &AgentAccess,
) -> Result<(String, Vec<String>)> {
    let mut keys = vec![
        "PATH",
        "LANG",
        "LC_ALL",
        "LC_CTYPE",
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "NO_PROXY",
        "http_proxy",
        "https_proxy",
        "all_proxy",
        "no_proxy",
        "RUSTUP_HOME",
    ];
    if access.codex_credentials {
        keys.extend(["OPENAI_API_KEY", "OPENAI_BASE_URL", "OPENAI_API_BASE"]);
    }
    for key in &access.env_allowlist {
        agent_policy::validate_environment_name(key)?;
        keys.push(key);
    }
    let host = HostAccess {
        home: std::env::var_os("HOME").map(PathBuf::from),
        codex_home: codex_configuration_home(),
        environment: keys
            .into_iter()
            .filter_map(|key| std::env::var(key).ok().map(|v| (key.into(), v)))
            .collect(),
        access: access.clone(),
    };
    wrap_with_host(program, args, state_dir, worktree, log_dir, write, host)
}

/// Start task-owned target services in the same namespace as Codex. Build the
/// mounts using the actual Codex executable so its distribution/helper survives
/// masking, then insert the trusted controller launcher before that executable.
pub fn wrap_agent_with_launcher(
    program: &str,
    args: &[String],
    state_dir: &Path,
    worktree: &Path,
    log_dir: &Path,
    write: bool,
    launcher: (&Path, &[String]),
) -> Result<(String, Vec<String>)> {
    wrap_agent_with_launcher_access(
        program,
        args,
        state_dir,
        worktree,
        log_dir,
        write,
        launcher,
        &AgentAccess::codex(),
    )
}

#[allow(clippy::too_many_arguments)]
pub fn wrap_agent_with_launcher_access(
    program: &str,
    args: &[String],
    state_dir: &Path,
    worktree: &Path,
    log_dir: &Path,
    write: bool,
    launcher: (&Path, &[String]),
    access: &AgentAccess,
) -> Result<(String, Vec<String>)> {
    let launcher_path = executable(launcher.0.to_str().context("launcher path is not UTF-8")?)?;
    let logs = checked_directory(log_dir)?;
    ensure!(
        launcher_path.starts_with(&logs),
        "target launcher must belong to this task"
    );
    let (bwrap, mut wrapped) =
        wrap_agent_with_access(program, args, state_dir, worktree, log_dir, write, access)?;
    let command_start = wrapped.len() - args.len() - 1;
    let command = wrapped.split_off(command_start);
    ensure!(
        wrapped.pop().as_deref() == Some("--"),
        "sandbox command delimiter is absent"
    );
    // Restore the task log directory for output, then protect only the trusted
    // launcher file. This mount must precede bubblewrap's command delimiter.
    add_mount(&mut wrapped, "--ro-bind", &launcher_path, &launcher_path)?;
    wrapped.push("--".into());
    wrapped.push(utf8(&launcher_path)?);
    wrapped.extend_from_slice(launcher.1);
    wrapped.push("--".into());
    wrapped.extend(command);
    Ok((bwrap, wrapped))
}

struct HostAccess {
    home: Option<PathBuf>,
    codex_home: Option<PathBuf>,
    environment: Vec<(String, String)>,
    access: AgentAccess,
}

fn wrap_with_host(
    program: &str,
    args: &[String],
    state_dir: &Path,
    worktree: &Path,
    log_dir: &Path,
    write: bool,
    host: HostAccess,
) -> Result<(String, Vec<String>)> {
    let bwrap =
        executable("bwrap").context("bubblewrap is required; refusing unsandboxed execution")?;
    let program = executable(program)?;
    let state = checked_directory(state_dir)?;
    let worktree = checked_directory(worktree)?;
    let logs = checked_directory(log_dir)?;
    let worktrees = state.join("worktrees");
    let runs = state.join("runs");
    ensure!(
        worktree.starts_with(&worktrees) && worktree != worktrees,
        "agent worktree must be below state_dir/worktrees"
    );
    ensure!(
        logs.starts_with(&runs) && logs != runs,
        "agent logs must be below state_dir/runs"
    );
    ensure!(
        !logs.starts_with(&worktree) && !worktree.starts_with(&logs),
        "task worktree and logs must not overlap"
    );
    let tools = state.join("tools");
    if program.starts_with(&state) {
        ensure!(
            program.starts_with(&tools),
            "agent executable inside controller state must be under state_dir/tools"
        );
    }

    let home = logs.join("home");
    let codex_home = logs.join("codex-home");
    for directory in [
        &home,
        &codex_home,
        &logs.join("memory"),
        &logs.join("cache"),
        &logs.join("config"),
        &logs.join("state"),
    ] {
        fs::create_dir_all(directory)?;
        checked_directory(directory)?;
        fs::set_permissions(directory, fs::Permissions::from_mode(0o700))?;
    }
    let host_codex_home = host.codex_home;
    if host.access.codex_credentials
        && let Some(host) = &host_codex_home
    {
        // Preserve provider settings and authentication without allowing the
        // child to modify a shared Codex installation or another task's state.
        for filename in ["auth.json", "models_cache.json", "version.json"] {
            let source = host.join(filename);
            let destination = codex_home.join(filename);
            if let Ok(metadata) = fs::symlink_metadata(&destination) {
                ensure!(
                    metadata.is_file() && !metadata.file_type().is_symlink(),
                    "private Codex file must not be a symlink or special file"
                );
            } else if source.is_file() {
                write_private_file(&destination, &fs::read(source)?)?;
            }
        }
        let config = host.join("config.toml");
        if config.is_file() {
            let destination = codex_home.join("config.toml");
            write_private_file(
                &destination,
                sanitize_codex_config(&fs::read_to_string(config)?)?.as_bytes(),
            )?;
        }
    }

    let mut wrapped = vec![
        "--clearenv".into(),
        "--ro-bind".into(),
        "/".into(),
        "/".into(),
        "--unshare-user".into(),
        "--disable-userns".into(),
        "--unshare-pid".into(),
        "--unshare-ipc".into(),
        "--unshare-uts".into(),
        "--unshare-cgroup-try".into(),
        "--new-session".into(),
        "--die-with-parent".into(),
        "--cap-drop".into(),
        "ALL".into(),
        "--proc".into(),
        "/proc".into(),
        "--dev".into(),
        "/dev".into(),
        "--tmpfs".into(),
        "/tmp".into(),
        "--perms".into(),
        "1777".into(),
        "--dir".into(),
        "/tmp/.X11-unix".into(),
        "--tmpfs".into(),
        "/run".into(),
        "--tmpfs".into(),
        utf8(&state)?,
    ];
    if !host.access.codex_credentials
        && let Some(host_home) = &host.home
    {
        // Generic adapters receive no ambient home directory: credentials may
        // live at arbitrary paths, not only in known provider directories.
        ensure!(
            !host_home.starts_with(&tools),
            "host HOME overlaps restored tool directory"
        );
        let state_mask = wrapped.split_off(wrapped.len() - 2);
        mask_credentials(&mut wrapped, host_home)?;
        wrapped.extend(state_mask);
        for relative in [".rustup", ".cargo/bin"] {
            let directory = host_home.join(relative);
            if directory.is_dir() {
                let directory = checked_directory(&directory)?;
                ensure!(
                    !directory.starts_with(&state),
                    "host toolchain aliases controller state"
                );
                add_mount(&mut wrapped, "--ro-bind", &directory, &directory)?;
            }
        }
    }
    add_mount(
        &mut wrapped,
        if write { "--bind" } else { "--ro-bind" },
        &worktree,
        &worktree,
    )?;
    add_mount(&mut wrapped, "--bind", &logs, &logs)?;
    // These host-created, immutable source worktrees contain the current checked
    // upstream code, never experiment logs, memory, holdouts, or delivery authority.
    let sources = state.join("sources");
    if sources.exists() {
        let sources = checked_directory(&sources)?;
        add_mount(&mut wrapped, "--ro-bind", &sources, &sources)?;
    }
    if tools.exists() {
        let tools = checked_directory(&tools)?;
        add_mount(&mut wrapped, "--ro-bind", &tools, &tools)?;
    }
    // Model authentication is copied into the private Codex home; the original
    // authentication/configuration trees are unavailable to the child.
    if let Some(host_home) = &host.home {
        for relative in [
            ".ssh",
            ".codex",
            ".config/gh",
            ".config/git",
            ".git-credentials",
            ".gitconfig",
            ".netrc",
            ".Xauthority",
            ".ICEauthority",
            // Browser management cookies and bootstrap history must remain host-only.
            ".mozilla",
            ".config/chromium",
            ".config/google-chrome",
            ".config/google-chrome-beta",
            ".config/microsoft-edge",
            ".config/BraveSoftware",
            "snap/firefox/common/.mozilla",
            ".local/share/keyrings",
        ] {
            mask_credentials(&mut wrapped, &host_home.join(relative))?;
        }
    }
    if let Some(host) = host_codex_home {
        mask_credentials(&mut wrapped, &host)?;
        let skills = host.join("skills");
        if skills.is_dir() {
            let resolved_skills = skills.canonicalize()?;
            ensure!(
                !resolved_skills.starts_with(&state) || resolved_skills.starts_with(&tools),
                "host skills cannot alias private controller state"
            );
            // Existing tool adapters and skill documentation may use the
            // original absolute installation path. Restore only that subtree,
            // leaving the original auth/config files masked.
            add_mount(&mut wrapped, "--ro-bind", &skills, &skills)?;
            add_mount(
                &mut wrapped,
                "--ro-bind",
                &skills,
                &codex_home.join("skills"),
            )?;
        }
    }
    // Standalone Codex installations resolve into CODEX_HOME/packages, which
    // was deliberately masked above. Restore exactly the selected executable
    // after all masks, including for /tmp-based test providers. Other package
    // files and original configuration/authentication remain hidden.
    add_mount(&mut wrapped, "--ro-bind", &program, &program)?;
    // The standalone distribution launches this exact sibling for tool calls.
    // Exposing the main executable alone permits model turns but breaks tools.
    if host.access.codex_credentials && program.file_name().is_some_and(|name| name == "codex") {
        let helper = program
            .parent()
            .context("executable has no parent")?
            .join("codex-code-mode-host");
        if helper.is_file() {
            let source = helper.canonicalize()?;
            ensure!(
                fs::metadata(&source)?.permissions().mode() & 0o111 != 0,
                "Codex tool host is not executable"
            );
            ensure!(
                !source.starts_with(&state) || source.starts_with(&tools),
                "Codex tool host cannot alias private controller state"
            );
            // Preserve the expected sibling pathname even when an installation
            // uses a symlink to the actual helper executable.
            add_mount(&mut wrapped, "--ro-bind", &source, &helper)?;
        }
    }
    // Codex desktop distributions may put ripgrep under the masked package
    // tree. Preserve that exact utility, just like the selected tool host.
    if let Ok(rg) = executable("rg") {
        ensure!(
            !rg.starts_with(&state) || rg.starts_with(&tools),
            "search utility cannot alias private controller state"
        );
        add_mount(&mut wrapped, "--ro-bind", &rg, &rg)?;
    }
    for view in &host.access.read_only_views {
        let view = checked_directory(view)?;
        let public_root = state.join("communication/views");
        let inbox_root = state.join("communication/inboxes");
        let cohort = if view.parent() == Some(public_root.as_path()) {
            Some(view.as_path())
        } else if logs.parent() == Some(runs.as_path())
            && view.file_name() == logs.file_name()
            && view.parent().and_then(Path::parent) == Some(inbox_root.as_path())
        {
            view.parent()
        } else {
            None
        };
        ensure!(
            cohort
                .and_then(Path::file_name)
                .and_then(|v| v.to_str())
                .is_some_and(|name| name.len() == 64
                    && name
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))),
            "read-only view must be an exported public cohort or this run's exact inbox"
        );
        add_mount(&mut wrapped, "--ro-bind", &view, &view)?;
    }
    if host.access.codex_credentials {
        wrapped.extend(["--setenv".into(), "CODEX_HOME".into(), utf8(&codex_home)?]);
    }
    for (key, value) in host.environment {
        wrapped.extend(["--setenv".into(), key, value]);
    }
    for (key, value) in [
        ("HOME", home),
        ("CARGO_HOME", logs.join("home/.cargo")),
        ("CARGO_TARGET_DIR", logs.join("cache/cargo-target")),
        ("RELAY_MEMORY_HOME", logs.join("memory")),
        ("RELAY_KNOWLEDGE_HOME", logs.join("knowledge-index")),
        ("XDG_CACHE_HOME", logs.join("cache")),
        ("XDG_CONFIG_HOME", logs.join("config")),
        ("XDG_STATE_HOME", logs.join("state")),
    ] {
        wrapped.extend(["--setenv".into(), key.into(), utf8(&value)?]);
    }
    for key in ["TMPDIR", "TMP", "TEMP"] {
        wrapped.extend(["--setenv".into(), key.into(), "/tmp".into()]);
    }
    // A different download URL must not invalidate an identical, checksum-
    // verified archive. The task still owns every mutable cache/lock file.
    wrapped.extend([
        "--setenv".into(),
        "BAZEL_HTTP_RULES_URLS_AS_DEFAULT_CANONICAL_ID".into(),
        "0".into(),
    ]);
    wrapped.extend([
        "--chdir".into(),
        utf8(&worktree)?,
        "--".into(),
        utf8(&program)?,
    ]);
    wrapped.extend_from_slice(args);
    Ok((utf8(&bwrap)?, wrapped))
}

fn mask_credentials(args: &mut Vec<String>, path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let path = path.canonicalize()?;
    if path.is_dir() {
        args.extend(["--tmpfs".into(), utf8(&path)?]);
    } else {
        add_mount(args, "--ro-bind", Path::new("/dev/null"), &path)?;
    }
    Ok(())
}

fn write_private_file(path: &Path, bytes: &[u8]) -> Result<()> {
    fs::check_write_bound(path, bytes.len() as u64)?;
    if let Ok(metadata) = fs::symlink_metadata(path) {
        ensure!(
            metadata.is_file() && !metadata.file_type().is_symlink(),
            "private Codex destination must be a regular file"
        );
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .custom_flags(libc::O_NOFOLLOW)
        .mode(0o600)
        .open(path)?;
    fs::set_file_permissions(&file, path, fs::Permissions::from_mode(0o600))?;
    fs::write_all(&mut file, path, bytes)?;
    fs::sync_all(&file, path)?;
    Ok(())
}

fn sanitize_codex_config(contents: &str) -> Result<String> {
    let source: toml::Value =
        toml::from_str(contents).context("invalid host Codex configuration")?;
    let source = source
        .as_table()
        .context("Codex configuration must be a table")?;
    let mut sanitized = toml::map::Map::new();
    sanitized.insert(
        "approval_policy".into(),
        agent_policy::APPROVAL_POLICY.into(),
    );
    sanitized.insert("sandbox_mode".into(), agent_policy::SANDBOX_MODE.into());
    for key in [
        "model",
        "model_provider",
        "model_reasoning_effort",
        "model_reasoning_summary",
        "model_verbosity",
        "preferred_auth_method",
    ] {
        if let Some(value) = source.get(key) {
            sanitized.insert(key.into(), value.clone());
        }
    }
    if let Some(providers) = source
        .get("model_providers")
        .and_then(toml::Value::as_table)
    {
        let mut selected = toml::map::Map::new();
        for (name, provider) in providers {
            let Some(provider) = provider.as_table() else {
                continue;
            };
            let mut fields = toml::map::Map::new();
            for key in [
                "name",
                "base_url",
                "env_key",
                "env_key_instructions",
                "wire_api",
                "query_params",
                "http_headers",
                "env_http_headers",
                "request_max_retries",
                "stream_max_retries",
                "stream_idle_timeout_ms",
                "requires_openai_auth",
                "supports_websockets",
            ] {
                if let Some(value) = provider.get(key) {
                    fields.insert(key.into(), value.clone());
                }
            }
            selected.insert(name.clone(), toml::Value::Table(fields));
        }
        sanitized.insert("model_providers".into(), toml::Value::Table(selected));
    }
    Ok(toml::to_string(&toml::Value::Table(sanitized))?)
}

/// Enforce the storage convention before preparing real held-out tasks. This
/// checks the canonical path, not a textual starts_with susceptible to symlinks.
pub fn validate_private_path(state_dir: &Path, path: &Path) -> Result<PathBuf> {
    let state = checked_directory(state_dir)?;
    let private = state.join("private");
    let resolved = checked_path(path)?;
    ensure!(
        resolved.starts_with(&private),
        "holdouts must be stored under state_dir/private"
    );
    Ok(resolved)
}

fn add_mount(args: &mut Vec<String>, mode: &str, source: &Path, destination: &Path) -> Result<()> {
    args.extend([mode.into(), utf8(source)?, utf8(destination)?]);
    Ok(())
}

fn utf8(path: &Path) -> Result<String> {
    path.to_str()
        .map(str::to_owned)
        .context("sandbox paths must be UTF-8")
}

fn checked_directory(path: &Path) -> Result<PathBuf> {
    let path = checked_path(path)?;
    ensure!(path.is_dir(), "sandbox path must be a directory");
    Ok(path)
}

fn checked_path(path: &Path) -> Result<PathBuf> {
    ensure!(path.is_absolute(), "sandbox paths must be absolute");
    let mut current = PathBuf::new();
    for component in path.components() {
        ensure!(
            !matches!(component, Component::ParentDir | Component::CurDir),
            "sandbox path must be canonical"
        );
        current.push(component.as_os_str());
        ensure!(
            !fs::symlink_metadata(&current)?.file_type().is_symlink(),
            "sandbox path cannot traverse a symlink"
        );
    }
    path.canonicalize().context("sandbox path must exist")
}

/// Resolve an executable through the host PATH and validate its canonical file.
/// This does not launch it or grant access to an agent namespace.
pub fn executable(program: &str) -> Result<PathBuf> {
    let candidate = if program.contains('/') {
        PathBuf::from(program)
    } else {
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .map(|directory| directory.join(program))
            .find(|path| {
                path.is_file()
                    && fs::metadata(path).is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
            })
            .with_context(|| format!("executable unavailable: {program}"))?
    };
    let canonical = candidate.canonicalize()?;
    ensure!(
        canonical.is_file() && fs::metadata(&canonical)?.permissions().mode() & 0o111 != 0,
        "program is not executable"
    );
    Ok(canonical)
}

fn codex_configuration_home() -> Option<PathBuf> {
    std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".codex")))
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
