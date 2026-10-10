//! Linux mount and PID isolation for task agents. Host files are read-only;
//! controller state is hidden, except the task's own worktree/logs and the
//! read-only tool installation. Network access remains available to model APIs.

use anyhow::{Context, Result, ensure};
use std::fs;
use std::io::Write;
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
/// exported cohort directories; controller authority is not mountable here.
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
        crate::agent_backend::validate_environment_name(key)?;
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
        let name = view
            .file_name()
            .and_then(|v| v.to_str())
            .context("invalid cohort view")?;
        ensure!(
            view.parent() == Some(state.join("communication/views").as_path())
                && name.len() == 64
                && name
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "read-only view must be an exact exported cohort directory"
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
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    file.write_all(bytes)?;
    file.sync_all()?;
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
        crate::agent_policy::APPROVAL_POLICY.into(),
    );
    sanitized.insert(
        "sandbox_mode".into(),
        crate::agent_policy::SANDBOX_MODE.into(),
    );
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

pub(crate) fn executable(program: &str) -> Result<PathBuf> {
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
mod tests {
    use super::*;
    use std::process::Command;

    fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf, PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let state = temp.path().join("lab-state");
        let worktree = state.join("worktrees/task-one");
        let logs = state.join("runs/task-one");
        for path in [
            &worktree,
            &logs,
            &state.join("private"),
            &state.join("runs/task-two"),
            &state.join("tools"),
        ] {
            fs::create_dir_all(path).unwrap();
        }
        (temp, state, worktree, logs)
    }

    #[test]
    fn rejects_symlink_aliases_and_worktrees_outside_private_state() {
        let (temp, state, worktree, logs) = fixture();
        assert!(wrap_agent("/bin/true", &[], &state, temp.path(), &logs, true).is_err());
        let alias = state.join("worktrees/alias");
        std::os::unix::fs::symlink(&worktree, &alias).unwrap();
        assert!(wrap_agent("/bin/true", &[], &state, &alias, &logs, true).is_err());
        assert!(validate_private_path(&state, &worktree).is_err());
        validate_private_path(&state, &state.join("private")).unwrap();
    }

    #[test]
    fn sandbox_hides_sibling_state_and_limits_writes_to_the_task() {
        let (_temp, state, worktree, logs) = fixture();
        let secret = state.join("private/answers.txt");
        let sibling = state.join("runs/task-two/report.txt");
        let source = state.join("tools/host-source.txt");
        let current_source = state.join("sources/latest/code.txt");
        fs::create_dir_all(current_source.parent().unwrap()).unwrap();
        fs::write(&current_source, "latest pinned source").unwrap();
        fs::write(&secret, "private holdout").unwrap();
        fs::write(&sibling, "another agent").unwrap();
        fs::write(&source, "unchanged").unwrap();
        std::os::unix::fs::symlink(&secret, worktree.join("answer-link")).unwrap();
        let script = "set -eu; test ! -e \"$1\"; test ! -e \"$2\"; test ! -e answer-link; if printf changed > \"$3\" 2>/dev/null; then exit 9; fi; printf own > artifact.txt; printf own-log > \"$4\"; test -d /proc/1; test \"$RELAY_MEMORY_HOME\" = \"$5\"; test \"$(cat \"$6\")\" = 'latest pinned source'; if printf changed > \"$6\" 2>/dev/null; then exit 10; fi";
        let args = vec![
            "-c".into(),
            script.into(),
            "sandbox-test".into(),
            utf8(&secret).unwrap(),
            utf8(&sibling).unwrap(),
            utf8(&source).unwrap(),
            utf8(&logs.join("artifact.txt")).unwrap(),
            utf8(&logs.join("memory")).unwrap(),
            utf8(&current_source).unwrap(),
        ];
        let (program, args) = wrap_agent("/bin/sh", &args, &state, &worktree, &logs, true).unwrap();
        let output = Command::new(program).args(args).output().unwrap();
        assert!(
            output.status.success(),
            "bubblewrap isolation unavailable or failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            fs::read_to_string(worktree.join("artifact.txt")).unwrap(),
            "own"
        );
        assert_eq!(
            fs::read_to_string(logs.join("artifact.txt")).unwrap(),
            "own-log"
        );
        assert_eq!(fs::read_to_string(source).unwrap(), "unchanged");
    }

    #[test]
    fn read_only_task_cannot_edit_its_worktree() {
        let (_temp, state, worktree, logs) = fixture();
        let args = vec!["-c".into(), "if printf changed > forbidden.txt 2>/dev/null; then exit 9; fi; printf report > \"$1\"".into(), "sandbox-test".into(), utf8(&logs.join("result.txt")).unwrap()];
        let (program, args) =
            wrap_agent("/bin/sh", &args, &state, &worktree, &logs, false).unwrap();
        let output = Command::new(program).args(args).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!worktree.join("forbidden.txt").exists());
        assert!(logs.join("result.txt").exists());
    }

    #[test]
    fn generic_backend_hides_entire_host_home_and_exposes_only_exact_read_only_view() {
        let (temp, state, worktree, logs) = fixture();
        let host_home = temp.path().join("host-home");
        let host_codex = host_home.join(".codex");
        fs::create_dir_all(host_home.join(".aws")).unwrap();
        fs::create_dir_all(host_codex.join("skills/example")).unwrap();
        fs::write(host_home.join(".aws/credentials"), "private-fixture").unwrap();
        fs::write(host_codex.join("auth.json"), "private-fixture").unwrap();
        fs::write(host_codex.join("skills/example/SKILL.md"), "public fixture").unwrap();
        let program = host_home.join("bridge");
        fs::write(
            &program,
            r#"#!/bin/sh
set -eu
test -z "${OPENAI_API_KEY+x}"
test -z "${GH_TOKEN+x}"
test -z "${CODEX_HOME+x}"
test "$TEST_MODEL_CREDENTIAL" = explicit-fixture
test ! -e "$1/.aws/credentials"
test ! -e "$1/.codex/auth.json"
test ! -e "$HOME/../codex-home/auth.json"
test -f "$1/.codex/skills/example/SKILL.md"
test "$(cat "$2/board.json")" = public-proposal
! touch "$2/forbidden" 2>/dev/null
test ! -e "$3/authority/private.json"
"#,
        )
        .unwrap();
        fs::set_permissions(&program, fs::Permissions::from_mode(0o700)).unwrap();
        let view = state.join("communication/views").join("a".repeat(64));
        fs::create_dir_all(&view).unwrap();
        fs::write(view.join("board.json"), "public-proposal").unwrap();
        fs::create_dir_all(state.join("communication/authority")).unwrap();
        fs::write(
            state.join("communication/authority/private.json"),
            "host-only",
        )
        .unwrap();
        let access = AgentAccess {
            codex_credentials: false,
            env_allowlist: vec![],
            read_only_views: vec![view.clone()],
        };
        let host = || HostAccess {
            home: Some(host_home.clone()),
            codex_home: Some(host_codex.clone()),
            environment: vec![
                ("PATH".into(), "/usr/bin:/bin".into()),
                ("TEST_MODEL_CREDENTIAL".into(), "explicit-fixture".into()),
            ],
            access: access.clone(),
        };
        let (binary, args) = wrap_with_host(
            program.to_str().unwrap(),
            &[
                utf8(&host_home).unwrap(),
                utf8(&view).unwrap(),
                utf8(&state.join("communication")).unwrap(),
            ],
            &state,
            &worktree,
            &logs,
            false,
            host(),
        )
        .unwrap();
        let output = Command::new(binary)
            .args(args)
            .env("OPENAI_API_KEY", "must-not-inherit")
            .env("GH_TOKEN", "must-not-inherit")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let mut invalid = host();
        invalid.access.read_only_views = vec![state.join("communication")];
        assert!(
            wrap_with_host(
                program.to_str().unwrap(),
                &[],
                &state,
                &worktree,
                &logs,
                false,
                invalid
            )
            .is_err()
        );
    }

    #[test]
    fn credentials_and_external_tool_configuration_are_not_inherited() {
        let (_temp, state, worktree, logs) = fixture();
        let host_home = state.join("tools/host-home");
        let gh = host_home.join(".config/gh");
        let host_codex = host_home.join(".codex");
        fs::create_dir_all(&gh).unwrap();
        fs::create_dir_all(host_codex.join("skills/example")).unwrap();
        fs::write(gh.join("hosts.yml"), "nonsecret-github-fixture").unwrap();
        fs::write(host_codex.join("auth.json"), "{\"fixture\":true}").unwrap();
        fs::write(host_codex.join("skills/example/SKILL.md"), "test skill").unwrap();
        fs::write(host_codex.join("config.toml"), "model = 'test-model'\nmodel_provider = 'local'\n[mcp_servers.external]\ncommand = 'remote-tool'\n[hooks]\ncommand = 'external-write'\n[model_providers.local]\nname = 'Local'\nbase_url = 'https://example.invalid/v1'\nwire_api = 'responses'\n").unwrap();
        let args = vec!["-c".into(), "set -eu; test -z \"${GH_TOKEN+x}\"; test -z \"${SSH_AUTH_SOCK+x}\"; test ! -e \"$1/hosts.yml\"; test ! -e \"$2/auth.json\"; test -f \"$CODEX_HOME/auth.json\"; test -f \"$CODEX_HOME/skills/example/SKILL.md\"; test -f \"$2/skills/example/SKILL.md\"".into(), "sandbox-test".into(), utf8(&gh).unwrap(), utf8(&host_codex).unwrap()];
        let access = HostAccess {
            home: Some(host_home),
            codex_home: Some(host_codex),
            environment: vec![("PATH".into(), "/usr/bin:/bin".into())],
            access: AgentAccess::codex(),
        };
        let (program, args) =
            wrap_with_host("/bin/sh", &args, &state, &worktree, &logs, false, access).unwrap();
        let output = Command::new(program)
            .args(args)
            .env("GH_TOKEN", "must-not-be-inherited")
            .env("SSH_AUTH_SOCK", "/unavailable/agent.sock")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let config: toml::Value =
            toml::from_str(&fs::read_to_string(logs.join("codex-home/config.toml")).unwrap())
                .unwrap();
        assert_eq!(config["model"].as_str(), Some("test-model"));
        assert_eq!(config["approval_policy"].as_str(), Some("never"));
        assert_eq!(config["sandbox_mode"].as_str(), Some("danger-full-access"));
        assert!(config.get("mcp_servers").is_none());
        assert!(config.get("hooks").is_none());
        assert_eq!(
            config["model_providers"]["local"]["wire_api"].as_str(),
            Some("responses")
        );
    }

    #[test]
    fn private_config_staging_never_follows_agent_created_symlinks() {
        let (temp, state, worktree, logs) = fixture();
        let outside = temp.path().join("host-config.toml");
        fs::write(&outside, "unchanged").unwrap();
        let alias = logs.join("config-link");
        std::os::unix::fs::symlink(&outside, &alias).unwrap();
        assert!(write_private_file(&alias, b"changed").is_err());
        assert_eq!(fs::read_to_string(&outside).unwrap(), "unchanged");
        let directory = temp.path().join("host-home");
        fs::create_dir(&directory).unwrap();
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o755)).unwrap();
        std::os::unix::fs::symlink(&directory, logs.join("home")).unwrap();
        assert!(wrap_agent("/bin/true", &[], &state, &worktree, &logs, false).is_err());
        assert_eq!(
            fs::metadata(directory).unwrap().permissions().mode() & 0o777,
            0o755
        );
    }

    #[test]
    fn exact_executable_survives_masked_codex_package_directory() {
        let (_temp, state, worktree, logs) = fixture();
        let host_home = state.join("tools/host-home");
        let host_codex = host_home.join(".codex");
        let package = host_codex.join("packages/release/bin");
        fs::create_dir_all(&package).unwrap();
        let executable = package.join("codex");
        let helper = package.join("codex-code-mode-host");
        let adjacent = package.join("must-remain-hidden");
        fs::write(&adjacent, "private fixture").unwrap();
        fs::write(
            &executable,
            "#!/bin/sh\nset -eu\nexec \"${0%/*}/codex-code-mode-host\" \"$1\"\n",
        )
        .unwrap();
        fs::write(
            &helper,
            "#!/bin/sh\nset -eu\ntest ! -e \"$1\"\nprintf 'fixture-tool-host\\n'\n",
        )
        .unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
        fs::set_permissions(&helper, fs::Permissions::from_mode(0o755)).unwrap();
        let access = HostAccess {
            home: Some(host_home),
            codex_home: Some(host_codex),
            environment: vec![("PATH".into(), "/usr/bin:/bin".into())],
            access: AgentAccess::codex(),
        };
        let (program, args) = wrap_with_host(
            executable.to_str().unwrap(),
            &[utf8(&adjacent).unwrap()],
            &state,
            &worktree,
            &logs,
            false,
            access,
        )
        .unwrap();
        let output = Command::new(program).args(args).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            "fixture-tool-host"
        );
    }

    #[test]
    #[ignore = "requires an installed Codex; run explicitly to verify the actual installation layout"]
    fn actual_installed_codex_version_inside_sandbox() {
        let (_temp, state, worktree, logs) = fixture();
        let (program, args) = wrap_agent(
            "codex",
            &["--version".into()],
            &state,
            &worktree,
            &logs,
            false,
        )
        .unwrap();
        let output = Command::new(program).args(args).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout)
                .to_lowercase()
                .contains("codex")
        );
    }
}
