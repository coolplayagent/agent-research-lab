//! Publish the reviewed commit only after worktree and committed snapshots agree.
use crate::{
    host::{git, relay},
    protocol::PublishReportRequest,
    snapshot::{allowed_publication_path, publication_snapshot, validate_prepared_publication},
};
use anyhow::{Result, ensure};
use delivery::{PullRequestReceipt, PullRequestRequest, capture_text, ensure_pull_request};
use std::path::PathBuf;

/// Publish only reviewed files and open a pull request in the configured repository.
pub fn publish_report(request: &PublishReportRequest) -> Result<PullRequestReceipt> {
    let prepared = &request.prepared;
    validate_prepared_publication(request)?;
    let remote = git(&prepared.worktree, &["remote", "get-url", "origin"])?;
    ensure!(
        remote == prepared.layout.expected_origin,
        "publication origin changed after review"
    );
    relay(
        &prepared.relay_knowledge_binary,
        &prepared.worktree,
        &["map", "validate", "--format", "json"],
    )?;
    let output = process::capture(
        "git",
        &[
            "status".into(),
            "--porcelain=v1".into(),
            "-z".into(),
            "--untracked-files=all".into(),
        ],
        &prepared.worktree,
        std::time::Duration::from_secs(30),
    )?;
    ensure!(
        output.status.success(),
        "cannot read publication worktree status"
    );
    let status = String::from_utf8(output.stdout)?;
    let mut changed = Vec::new();
    for record in status.split('\0').filter(|s| !s.is_empty()) {
        ensure!(
            record.len() >= 4 && !record[..2].contains(['R', 'C']),
            "renames require explicit publication review"
        );
        let path = PathBuf::from(&record[3..]);
        ensure!(
            allowed_publication_path(&path, prepared),
            "unexpected publication file: {}",
            path.display()
        );
        changed.push(path);
    }
    if !changed.is_empty() {
        let mut args = vec!["add".into(), "--".into()];
        args.extend(changed.iter().map(|p| p.to_string_lossy().into_owned()));
        capture_text("git", &args, &prepared.worktree)?;
        git(&prepared.worktree, &["commit", "-m", &request.title])?;
    }
    let head = git(&prepared.worktree, &["rev-parse", "HEAD"])?;
    ensure!(
        head != prepared.baseline,
        "publication has no committed contribution"
    );
    ensure!(
        publication_snapshot(prepared, Some(&head))? == prepared.publication_files,
        "committed publication differs from reviewed snapshot"
    );
    git(
        &prepared.worktree,
        &[
            "push",
            "origin",
            &format!("{head}:refs/heads/{}", prepared.branch),
        ],
    )?;
    ensure_pull_request(&PullRequestRequest {
        repository: prepared.layout.repository.clone(),
        working_directory: prepared.worktree.clone(),
        branch: prepared.branch.clone(),
        base: request.base_branch.clone(),
        expected_head: head,
        title: request.title.clone(),
        body: request.body.clone(),
    })
}

#[cfg(test)]
#[path = "publication_tests.rs"]
mod tests;
