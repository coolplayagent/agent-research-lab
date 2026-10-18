//! Idempotent PR creation with exact pushed-head checks.
use crate::{
    github::{gh_json, pr_receipt},
    host::{capture_text, unique_id},
    protocol::{PullRequestReceipt, PullRequestRequest},
    validation::{validate_commit, validate_repository},
};
use anyhow::{Context, Result, ensure};
use std::{
    fs::{self, OpenOptions},
    io::Write,
};

/// The branch must already be pushed. A retry reuses its matching PR, including an already merged PR.
pub fn ensure_pull_request(request: &PullRequestRequest) -> Result<PullRequestReceipt> {
    validate_repository(&request.repository)?;
    validate_commit(&request.expected_head)?;
    for branch in [&request.branch, &request.base] {
        capture_text(
            "git",
            &["check-ref-format".into(), "--branch".into(), branch.clone()],
            &request.working_directory,
        )?;
    }
    ensure!(
        request.branch != request.base,
        "PR head and base must differ"
    );
    let values = gh_json(
        vec![
            "pr".into(),
            "list".into(),
            "--repo".into(),
            request.repository.clone(),
            "--head".into(),
            request.branch.clone(),
            "--base".into(),
            request.base.clone(),
            "--state".into(),
            "all".into(),
            "--json".into(),
            "number,url,state,headRefOid".into(),
            "--limit".into(),
            "100".into(),
        ],
        &request.working_directory,
    )?;
    let values = values.as_array().context("expected PR array")?;
    ensure!(
        values.len() < 100,
        "PR lookup is truncated; reconcile before creating another PR"
    );
    let matching: Vec<_> = values
        .iter()
        .filter(|v| v["headRefOid"].as_str() == Some(&request.expected_head))
        .collect();
    ensure!(
        matching.len() <= 1,
        "multiple PRs match the candidate; reconciliation required"
    );
    if let Some(value) = matching.first() {
        let receipt = pr_receipt(value, true)?;
        ensure!(
            receipt.state != contracts::PullRequestState::Closed.as_str(),
            "candidate PR was closed; do not recreate automatically"
        );
        return Ok(receipt);
    }
    ensure!(
        !values
            .iter()
            .any(|v| v["state"] == contracts::PullRequestState::Open.as_str()),
        "branch has an open PR at another head; evidence must be refreshed"
    );
    let reference = gh_json(
        vec![
            "api".into(),
            format!(
                "repos/{}/git/ref/heads/{}",
                request.repository, request.branch
            ),
        ],
        &request.working_directory,
    )?;
    ensure!(
        reference["object"]["sha"].as_str() == Some(&request.expected_head),
        "remote branch head differs from evaluated source"
    );
    let body_path = std::env::temp_dir().join(format!("agent-research-pr-{}.md", unique_id()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&body_path)?;
    file.write_all(request.body.as_bytes())?;
    let result = capture_text(
        "gh",
        &[
            "pr".into(),
            "create".into(),
            "--repo".into(),
            request.repository.clone(),
            "--head".into(),
            request.branch.clone(),
            "--base".into(),
            request.base.clone(),
            "--title".into(),
            request.title.clone(),
            "--body-file".into(),
            body_path.to_string_lossy().into_owned(),
        ],
        &request.working_directory,
    );
    let _ = fs::remove_file(body_path);
    let url = result?;
    let value = gh_json(
        vec![
            "pr".into(),
            "view".into(),
            url,
            "--repo".into(),
            request.repository.clone(),
            "--json".into(),
            "number,url,state,headRefOid".into(),
        ],
        &request.working_directory,
    )?;
    let receipt = pr_receipt(&value, false)?;
    ensure!(
        receipt.head == request.expected_head,
        "PR head moved while creating PR; re-evaluate before merging"
    );
    Ok(receipt)
}
