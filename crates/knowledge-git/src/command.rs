//! Bounded Git commands with an empty global configuration and explicit environment.
use anyhow::{Result, ensure};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

pub(super) fn path(path: &Path) -> Result<PathBuf> {
    #[cfg(windows)]
    {
        windows_runtime::user_path(path)
    }
    #[cfg(unix)]
    {
        Ok(path.into())
    }
}
pub(super) fn environment(env: &mut Vec<(String, String)>) {
    #[cfg(unix)]
    env.push(("PATH".into(), "/usr/bin:/bin".into()));
    #[cfg(windows)]
    for name in ["PATH", "SystemRoot", "TEMP", "TMP"] {
        if let Ok(value) = std::env::var(name) {
            env.push((name.into(), value));
        }
    }
}
pub(super) fn capture(
    args: &[String],
    cwd: &Path,
    output: &Path,
    timeout: Duration,
    env: &[(String, String)],
) -> Result<String> {
    #[cfg(unix)]
    {
        let mut child =
            process::Process::spawn_clean("/usr/bin/git", args, cwd, output, timeout, env, None)?;
        loop {
            if let Some(status) = child.poll()? {
                ensure!(status.success(), "knowledge Git operation failed");
                ensure!(
                    fs::metadata(&child.stdout)?.len() <= 2 * 1024 * 1024,
                    "knowledge output exceeds bound"
                );
                return Ok(fs::read_to_string(&child.stdout)?);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    #[cfg(windows)]
    {
        use anyhow::Context;
        let program = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .filter(|p| p.is_absolute())
            .map(|p| p.join("git.exe"))
            .find(|p| p.is_file())
            .context("knowledge_git_runtime_missing")?;
        let mut command = vec![path(&program)?.to_string_lossy().into_owned()];
        command.extend_from_slice(args);
        let stdout_path = output.join("stdout");
        let stderr_path = output.join("stderr");
        let stdout = fs::File::create(&stdout_path)?;
        let stderr = fs::File::create(&stderr_path)?;
        let stdin = fs::File::open("NUL")?;
        let child = windows_runtime::Process::spawn(
            None,
            &command,
            &path(cwd)?,
            env,
            false,
            &stdin,
            &stdout,
            &stderr,
        )?;
        let deadline = std::time::Instant::now() + timeout;
        loop {
            ensure!(
                std::time::Instant::now() < deadline,
                "knowledge operation deadline expired"
            );
            ensure!(
                stdout.metadata()?.len() <= 2 * 1024 * 1024
                    && stderr.metadata()?.len() <= 2 * 1024 * 1024,
                "knowledge output exceeds bound"
            );
            if let Some(code) = child.poll()? {
                ensure!(code == 0, "knowledge Git operation failed");
                drop(child);
                return Ok(fs::read_to_string(&stdout_path)?);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
