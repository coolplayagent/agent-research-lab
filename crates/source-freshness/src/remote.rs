//! Capture and recheck the actual remote default branch without moving the worktree.
use crate::{
    client::RemoteClient,
    identity::{github_origin, trusted_repository, url_component, validate_branch, validate_hex},
    protocol::RemoteSnapshot,
};
use anyhow::{Context, Result, ensure};
use chrono::Utc;
use std::path::Path;

pub fn remote_snapshot(repository_path: &Path) -> Result<RemoteSnapshot> {
    RemoteClient::default().remote_snapshot(repository_path)
}

pub fn verify_fresh(repository_path: &Path, snapshot: &RemoteSnapshot) -> Result<RemoteSnapshot> {
    RemoteClient::default().verify_fresh(repository_path, snapshot)
}

pub fn verify_remote_binding(snapshot: &RemoteSnapshot) -> Result<RemoteSnapshot> {
    RemoteClient::default().verify_remote_binding(snapshot)
}

impl RemoteClient {
    pub(super) fn default_branch(&self, repository: &str) -> Result<(String, String)> {
        trusted_repository(repository)?;
        let value = self.gh_json(&[
            "repo".into(),
            "view".into(),
            repository.into(),
            "--json".into(),
            "nameWithOwner,defaultBranchRef".into(),
        ])?;
        let actual = value["nameWithOwner"]
            .as_str()
            .context("GitHub repository identity missing")?;
        ensure!(
            actual.eq_ignore_ascii_case(repository),
            "GitHub returned another repository"
        );
        trusted_repository(actual)?;
        let branch = value["defaultBranchRef"]["name"]
            .as_str()
            .context("repository has no default branch")?;
        validate_branch(branch)?;
        Ok((actual.into(), branch.into()))
    }

    pub(super) fn remote_binding(&self, repository: &str) -> Result<RemoteSnapshot> {
        let (repository, branch) = self.default_branch(repository)?;
        let value = self.gh_json(&[
            "api".into(),
            format!("repos/{repository}/commits/{}", url_component(&branch)),
            "--jq".into(),
            "{sha:.sha}".into(),
        ])?;
        let commit = value["sha"]
            .as_str()
            .context("GitHub default branch commit missing")?;
        validate_hex(commit, 40)?;
        Ok(RemoteSnapshot {
            repository,
            default_branch: branch,
            commit: commit.into(),
            checked_at: Utc::now().to_rfc3339(),
        })
    }

    /// Query GitHub, then fetch precisely that default branch into a dedicated
    /// host ref. Neither the checked-out branch nor the worktree/index changes.
    pub fn remote_snapshot(&self, repository_path: &Path) -> Result<RemoteSnapshot> {
        let origin = self.checked(
            &self.git,
            &["config".into(), "--get".into(), "remote.origin.url".into()],
            repository_path,
        )?;
        let repository = github_origin(&origin)?;
        let snapshot = self.remote_binding(&repository)?;
        let reference = format!(
            "refs/agent-research-lab/default/{}",
            snapshot.default_branch
        );
        self.checked(
            &self.git,
            &["check-ref-format".into(), reference.clone()],
            repository_path,
        )?;
        self.checked(
            &self.git,
            &[
                "fetch".into(),
                "--no-tags".into(),
                "--no-write-fetch-head".into(),
                "origin".into(),
                format!("+refs/heads/{}:{reference}", snapshot.default_branch),
            ],
            repository_path,
        )?;
        let fetched = self.checked(
            &self.git,
            &[
                "rev-parse".into(),
                "--verify".into(),
                format!("{reference}^{{commit}}"),
            ],
            repository_path,
        )?;
        ensure!(
            fetched == snapshot.commit,
            "remote default branch moved during fetch; new snapshot required"
        );
        // Reject a default-branch switch or advance that raced the fetch.
        self.verify_remote_binding(&snapshot)
    }

    pub fn verify_fresh(
        &self,
        repository_path: &Path,
        snapshot: &RemoteSnapshot,
    ) -> Result<RemoteSnapshot> {
        snapshot.validate()?;
        let latest = self.remote_snapshot(repository_path)?;
        ensure!(
            snapshot.same_binding(&latest),
            "snapshot is stale; create a new experiment ID at the current remote default branch"
        );
        Ok(latest)
    }

    /// Read-only GitHub gate for publication/activation callers with no local clone.
    pub fn verify_remote_binding(&self, snapshot: &RemoteSnapshot) -> Result<RemoteSnapshot> {
        snapshot.validate()?;
        let latest = self.remote_binding(&snapshot.repository)?;
        ensure!(
            snapshot.same_binding(&latest),
            "snapshot no longer matches the remote default branch"
        );
        Ok(latest)
    }
}

#[cfg(test)]
#[path = "remote_tests.rs"]
mod tests;
