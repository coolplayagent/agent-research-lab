//! Stable release lookup; absence requires both explicit 404 and repository identity.
use crate::{client::RemoteClient, identity::trusted_repository};
use anyhow::{Context, Result, ensure};
use serde_json::Value;

impl RemoteClient {
    pub(super) fn latest_release(&self, repository: &str) -> Result<Option<(u64, String)>> {
        trusted_repository(repository)?;
        let result = self.command(
            &self.gh,
            &["api".into(), format!("repos/{repository}/releases/latest")],
            &std::env::current_dir()?,
        )?;
        if !result.status.success() {
            let body = serde_json::from_slice::<Value>(&result.stdout).ok();
            let explicit_404 = body
                .as_ref()
                .is_some_and(|v| v["status"] == "404" || v["status"] == 404)
                && String::from_utf8_lossy(&result.stderr).contains("HTTP 404");
            ensure!(
                explicit_404,
                "cannot verify latest published release: {}",
                String::from_utf8_lossy(&result.stderr)
            );
            // GitHub also uses 404 for absent/inaccessible repositories. Establish
            // that the public repository exists before interpreting no release.
            self.default_branch(repository)?;
            return Ok(None);
        }
        let value: Value = serde_json::from_slice(&result.stdout)?;
        ensure!(
            value["draft"] == false && value["prerelease"] == false,
            "latest release is not a stable published release"
        );
        let id = value["id"]
            .as_u64()
            .filter(|id| *id > 0)
            .context("latest release ID missing")?;
        let tag = value["tag_name"]
            .as_str()
            .filter(|tag| !tag.is_empty())
            .context("latest release tag missing")?;
        Ok(Some((id, tag.into())))
    }
}

#[cfg(test)]
#[path = "releases_tests.rs"]
mod tests;
