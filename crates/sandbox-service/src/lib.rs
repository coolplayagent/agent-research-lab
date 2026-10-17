use anyhow::{Context, Result, ensure};
use service_api::SandboxOutput;
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
const BWRAP: &str = "/usr/bin/bwrap";
const PRLIMIT: &str = "/usr/bin/prlimit";
// Native adapters maintain disposable databases and caches independently of
// their protocol output. Keep their file quota below the enclosing tmpfs bound.
const MAX_RUNTIME_FILE: u64 = 16 * 1024 * 1024;
const MAX_PROTOCOL_OUTPUT: u64 = 256 * 1024;
const MAX_DIAGNOSTIC_OUTPUT: u64 = 4 * 1024 * 1024;
fn arguments(
    command: &[String],
    mounts: &[PathBuf],
    network: bool,
    seconds: u64,
) -> Result<Vec<String>> {
    let mut args = vec![
        format!("--cpu={}:{}", seconds + 1, seconds + 2),
        "--as=4294967296".into(),
        format!("--fsize={MAX_RUNTIME_FILE}:{MAX_RUNTIME_FILE}"),
        "--nofile=256".into(),
        "--".into(),
        BWRAP.into(),
        "--die-with-parent".into(),
        "--new-session".into(),
        "--unshare-all".into(),
    ];
    if network {
        args.push("--share-net".into());
    }

    for path in ["/usr", "/bin", "/lib", "/lib64"] {
        if Path::new(path).exists() {
            args.extend(["--ro-bind".into(), path.into(), path.into()]);
        }
    }
    if network {
        for path in ["/etc/resolv.conf", "/etc/hosts", "/etc/ssl/certs"] {
            if Path::new(path).exists() {
                args.extend(["--ro-bind".into(), path.into(), path.into()]);
            }
        }
    }
    for path in mounts {
        let resolved = fs::canonicalize(path)?;
        ensure!(
            resolved.components().count() > 2
                && !["/proc", "/dev", "/sys", "/run"]
                    .iter()
                    .any(|p| resolved.starts_with(p)),
            "invalid read-only mount"
        );
        let path = resolved
            .to_str()
            .context("mount must be UTF-8")?
            .to_string();
        args.extend(["--ro-bind".into(), path.clone(), path]);
    }
    args.extend([
        "--proc".into(),
        "/proc".into(),
        "--dev".into(),
        "/dev".into(),
        "--size".into(),
        "268435456".into(),
        "--tmpfs".into(),
        "/tmp".into(),
        "--size".into(),
        "67108864".into(),
        "--tmpfs".into(),
        "/home".into(),
        "--dir".into(),
        "/home/agent".into(),
        "--size".into(),
        "268435456".into(),
        "--tmpfs".into(),
        "/workspace".into(),
        "--chdir".into(),
        "/workspace".into(),
        "--setenv".into(),
        "HOME".into(),
        "/home/agent".into(),
        "--setenv".into(),
        "PATH".into(),
        "/usr/local/bin:/usr/bin:/bin".into(),
    ]);
    args.push("--".into());
    args.extend_from_slice(command);
    Ok(args)
}
pub async fn probe(state: &Path) -> Result<()> {
    run(
        state,
        vec!["/usr/bin/true".into()],
        vec![],
        vec![],
        false,
        String::new(),
        3,
    )
    .await?;
    Ok(())
}
pub async fn run(
    state: &Path,
    command: Vec<String>,
    env_names: Vec<String>,
    read_only_paths: Vec<PathBuf>,
    network: bool,
    input: String,
    seconds: u64,
) -> Result<SandboxOutput> {
    let validation = service_api::Adapter {
        id: "sandbox-input".into(),
        agent: contracts::CodingAgent::Custom,
        protocol: contracts::AgentProtocol::JsonStdioV1,
        enabled: true,
        executor_id: "executor".into(),
        command: command.clone(),
        env_names: env_names.clone(),
        read_only_paths: read_only_paths.clone(),
        network,
        max_seconds: seconds,
    };
    validation.validate()?;
    ensure!(input.len() <= 256 * 1024, "sandbox input exceeds bound");
    let temp = tempfile::Builder::new().prefix("run-").tempdir_in(state)?;
    let work = temp.path().join("workspace");
    fs::create_dir(&work)?;
    let stdin = temp.path().join("input.json");
    fs::write(&stdin, input)?;
    let env = env_names
        .into_iter()
        .map(|name| {
            let value = std::env::var(&name).with_context(|| {
                format!("configured environment variable is unavailable: {name}")
            })?;
            Ok((name, value))
        })
        .collect::<Result<Vec<_>>>()?;
    let args = arguments(&command, &read_only_paths, network, seconds)?;
    let started = Instant::now();
    let mut child = process::Process::spawn_clean(
        PRLIMIT,
        &args,
        temp.path(),
        &temp.path().join("logs"),
        Duration::from_secs(seconds),
        &env,
        Some(&stdin),
    )?;
    loop {
        let status = child.poll()?;
        ensure!(
            fs::metadata(&child.stdout)?.len() <= MAX_PROTOCOL_OUTPUT,
            "sandbox protocol output exceeds bound"
        );
        ensure!(
            fs::metadata(&child.stderr)?.len() <= MAX_DIAGNOSTIC_OUTPUT,
            "sandbox diagnostic output exceeds bound"
        );
        if let Some(status) = status {
            ensure!(
                status.success(),
                "sandbox command failed with exit code {:?}",
                status.code()
            );
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let stdout = fs::read_to_string(&child.stdout)?;
    ensure!(
        stdout.len() as u64 <= MAX_PROTOCOL_OUTPUT,
        "sandbox protocol output exceeds bound"
    );
    Ok(SandboxOutput {
        stdout,
        elapsed_ms: started.elapsed().as_millis() as u64,
        sandbox_id: "bubblewrap-v1".into(),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sandbox_mounts_are_read_only_and_work_is_isolated() {
        let args = arguments(&["/usr/bin/true".into()], &[], false, 4).unwrap();
        assert!(args.contains(&"--unshare-all".into()));
        assert!(!args.contains(&"--share-net".into()));
        assert!(args.windows(2).any(|w| w == ["--tmpfs", "/workspace"]));
        assert!(!args.contains(&"/root".into()));
    }

    async fn python(state: &Path, script: &str) -> Result<SandboxOutput> {
        run(
            state,
            vec!["/usr/bin/python3".into(), "-c".into(), script.into()],
            vec![],
            vec![],
            false,
            String::new(),
            5,
        )
        .await
    }

    #[tokio::test]
    async fn native_runtime_files_are_bounded_separately_from_protocol_output() {
        let state = tempfile::tempdir().unwrap();
        let result = python(
            state.path(),
            "with open('/workspace/runtime.db','wb') as f: f.write(b'x'*(8*1024*1024))\nprint('runtime file accepted')",
        )
        .await
        .unwrap();
        assert_eq!(result.stdout, "runtime file accepted\n");
        assert!(
            python(
                state.path(),
                "with open('/workspace/runtime.db','wb') as f: f.write(b'x'*(16*1024*1024+1))"
            )
            .await
            .is_err()
        );
        let oversized = python(
            state.path(),
            "import sys; sys.stdout.write('x'*(256*1024+1))",
        )
        .await
        .err()
        .unwrap();
        assert!(
            oversized
                .to_string()
                .contains("protocol output exceeds bound")
        );
        let diagnostics = python(
            state.path(),
            "import sys; sys.stderr.write('x'*(4*1024*1024+1))",
        )
        .await
        .err()
        .unwrap();
        assert!(
            diagnostics
                .to_string()
                .contains("diagnostic output exceeds bound")
        );
        assert!(python(state.path(), "raise SystemExit(7)").await.is_err());
    }
}
