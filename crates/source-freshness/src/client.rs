//! Configurable bounded subprocess transport for freshness adapters.
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Clone, Debug)]
pub struct RemoteClient {
    pub git: PathBuf,
    pub gh: PathBuf,
    pub timeout: Duration,
}

impl Default for RemoteClient {
    fn default() -> Self {
        Self {
            git: "git".into(),
            gh: "gh".into(),
            timeout: Duration::from_secs(90),
        }
    }
}

impl RemoteClient {
    pub(super) fn command(
        &self,
        binary: &Path,
        arguments: &[String],
        cwd: &Path,
    ) -> Result<std::process::Output> {
        process::capture(
            binary.to_str().context("non-UTF8 tool path")?,
            arguments,
            cwd,
            self.timeout,
        )
    }

    pub(super) fn checked(
        &self,
        binary: &Path,
        arguments: &[String],
        cwd: &Path,
    ) -> Result<String> {
        let output = self.command(binary, arguments, cwd)?;
        ensure!(
            output.status.success(),
            "freshness command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(String::from_utf8(output.stdout)?.trim().to_owned())
    }

    pub(super) fn gh_json(&self, arguments: &[String]) -> Result<Value> {
        let output = self.checked(&self.gh, arguments, &std::env::current_dir()?)?;
        serde_json::from_str(&output).context("GitHub freshness response was not JSON")
    }
}
