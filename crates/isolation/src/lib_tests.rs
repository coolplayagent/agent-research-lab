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
fn task_launcher_is_read_only_while_outputs_remain_writable() {
    let (_temp, state, worktree, logs) = fixture();
    let launcher = logs.join("trusted-launcher");
    let script = r#"#!/bin/sh
set -eu
test "$0" = "$1"
shift
/usr/bin/python3 -c 'import os,sys; assert os.statvfs(sys.argv[1]).f_flag & os.ST_RDONLY' "$0"
if chmod u+w "$0" 2>/dev/null; then exit 11; fi
if ( : > "$0" ) 2>/dev/null; then exit 12; fi
printf launcher-output > "$(dirname "$0")/launcher-output.txt"
test "$1" = --
shift
exec "$@"
"#;
    fs::write(&launcher, script).unwrap();
    // The host file is owner-writable: rejection inside the namespace must
    // come from the precise read-only mount, not Unix permission bits.
    fs::set_permissions(&launcher, fs::Permissions::from_mode(0o700)).unwrap();
    let access = AgentAccess {
        codex_credentials: false,
        env_allowlist: vec![],
        read_only_views: vec![],
    };
    let (program, args) = wrap_agent_with_launcher_access(
        "/bin/sh",
        &["-c".into(), "printf agent-output > agent-output.txt".into()],
        &state,
        &worktree,
        &logs,
        true,
        (&launcher, &[utf8(&launcher).unwrap()]),
        &access,
    )
    .unwrap();
    let output = Command::new(program).args(args).output().unwrap();
    assert!(
        output.status.success(),
        "launcher isolation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fs::read_to_string(&launcher).unwrap(), script);
    assert_eq!(
        fs::read_to_string(logs.join("launcher-output.txt")).unwrap(),
        "launcher-output"
    );
    assert_eq!(
        fs::read_to_string(worktree.join("agent-output.txt")).unwrap(),
        "agent-output"
    );
    assert!(
        wrap_agent_with_launcher_access(
            "/bin/true",
            &[],
            &state,
            &worktree,
            &logs,
            false,
            (Path::new("/bin/true"), &[]),
            &access,
        )
        .is_err()
    );
}

#[test]
fn read_only_task_cannot_edit_its_worktree() {
    let (_temp, state, worktree, logs) = fixture();
    let args = vec![
        "-c".into(),
        "if printf changed > forbidden.txt 2>/dev/null; then exit 9; fi; printf report > \"$1\""
            .into(),
        "sandbox-test".into(),
        utf8(&logs.join("result.txt")).unwrap(),
    ];
    let (program, args) = wrap_agent("/bin/sh", &args, &state, &worktree, &logs, false).unwrap();
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
fn private_inbox_mount_is_limited_to_the_current_run() {
    let (_temp, state, worktree, logs) = fixture();
    let cohort = state.join("communication/inboxes").join("b".repeat(64));
    let own = cohort.join("task-one");
    let peer = cohort.join("task-two");
    for directory in [&own, &peer] {
        fs::create_dir_all(directory).unwrap();
        fs::write(directory.join("board.json"), "private-proposal").unwrap();
    }
    let access = |view| AgentAccess {
        codex_credentials: false,
        env_allowlist: vec![],
        read_only_views: vec![view],
    };
    let (program, args) = wrap_agent_with_access(
            "/bin/sh",
            &[
                "-c".into(),
                "set -eu; test \"$(cat \"$1/board.json\")\" = private-proposal; ! touch \"$1/forbidden\" 2>/dev/null; test ! -e \"$2/board.json\"".into(),
                "inbox-test".into(),
                utf8(&own).unwrap(),
                utf8(&peer).unwrap(),
            ],
            &state,
            &worktree,
            &logs,
            false,
            &access(own.clone()),
        ).unwrap();
    let output = Command::new(program).args(args).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let alias = cohort.join("alias");
    std::os::unix::fs::symlink(&own, &alias).unwrap();
    for invalid in [peer, cohort.clone(), cohort.parent().unwrap().into(), alias] {
        assert!(
            wrap_agent_with_access(
                "/bin/true",
                &[],
                &state,
                &worktree,
                &logs,
                false,
                &access(invalid)
            )
            .is_err()
        );
    }
}

#[test]
fn credentials_and_external_tool_configuration_are_not_inherited() {
    let (_temp, state, worktree, logs) = fixture();
    let host_home = state.join("tools/host-home");
    let gh = host_home.join(".config/gh");
    let host_codex = host_home.join(".codex");
    for relative in [".mozilla", ".config/chromium", ".config/google-chrome"] {
        fs::create_dir_all(host_home.join(relative)).unwrap();
        fs::write(
            host_home.join(relative).join("cookies.sqlite"),
            "operator-cookie-fixture",
        )
        .unwrap();
    }
    fs::create_dir_all(&gh).unwrap();
    fs::create_dir_all(host_codex.join("skills/example")).unwrap();
    fs::write(gh.join("hosts.yml"), "nonsecret-github-fixture").unwrap();
    fs::write(host_codex.join("auth.json"), "{\"fixture\":true}").unwrap();
    fs::write(host_codex.join("skills/example/SKILL.md"), "test skill").unwrap();
    fs::write(host_codex.join("config.toml"), "model = 'test-model'\nmodel_provider = 'local'\n[mcp_servers.external]\ncommand = 'remote-tool'\n[hooks]\ncommand = 'external-write'\n[model_providers.local]\nname = 'Local'\nbase_url = 'https://example.invalid/v1'\nwire_api = 'responses'\n").unwrap();
    let args = vec!["-c".into(), "set -eu; test -z \"${GH_TOKEN+x}\"; test -z \"${SSH_AUTH_SOCK+x}\"; test ! -e \"$1/hosts.yml\"; test ! -e \"$2/auth.json\"; test -f \"$CODEX_HOME/auth.json\"; test -f \"$CODEX_HOME/skills/example/SKILL.md\"; test -f \"$2/skills/example/SKILL.md\"; test ! -e \"$2/../.mozilla/cookies.sqlite\"; test ! -e \"$2/../.config/chromium/cookies.sqlite\"; test ! -e \"$2/../.config/google-chrome/cookies.sqlite\"".into(), "sandbox-test".into(), utf8(&gh).unwrap(), utf8(&host_codex).unwrap()];
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
        toml::from_str(&fs::read_to_string(logs.join("codex-home/config.toml")).unwrap()).unwrap();
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
