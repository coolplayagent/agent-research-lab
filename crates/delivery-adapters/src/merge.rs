//! Refresh evidence before a head-fenced merge and report the observed outcome.
use crate::{
    github::{gh_json, github_snapshot, pr_receipt},
    host::capture_text,
    merge_gate::evaluate_merge_gate,
    protocol::{MergeRequest, PullRequestReceipt},
    validation::validate_commit,
};
use anyhow::{Context, Result, ensure};
use freshness::RemoteClient;

pub fn merge_pull_request(request: &MergeRequest) -> Result<PullRequestReceipt> {
    validate_commit(&request.expected_head)?;
    let snapshot = github_snapshot(request)?;
    if snapshot.state == contracts::PullRequestState::Merged.as_str() {
        ensure!(
            snapshot.head == request.expected_head,
            "already merged PR has another head"
        );
        let value = gh_json(
            vec![
                "pr".into(),
                "view".into(),
                request.number.to_string(),
                "--repo".into(),
                request.repository.clone(),
                "--json".into(),
                "number,url,state,headRefOid".into(),
            ],
            &request.working_directory,
        )?;
        return pr_receipt(&value, true);
    }
    evaluate_merge_gate(request, &snapshot)?;
    request
        .evidence
        .skills_manifest
        .as_ref()
        .context("missing release skills manifest")?
        .verify_skills(&RemoteClient::default())?;
    // Skill checks can be slow. Refresh PR/base/check state after them, then
    // perform the latest default-ref check immediately before requesting merge.
    let final_snapshot = github_snapshot(request)?;
    evaluate_merge_gate(request, &final_snapshot)?;
    freshness::verify_remote_binding(
        request
            .evidence
            .upstream
            .as_ref()
            .context("missing release upstream baseline")?,
    )?;
    capture_text(
        "gh",
        &[
            "pr".into(),
            "merge".into(),
            request.number.to_string(),
            "--repo".into(),
            request.repository.clone(),
            "--squash".into(),
            "--match-head-commit".into(),
            request.expected_head.clone(),
        ],
        &request.working_directory,
    )?;
    // GitHub may enqueue a merge. Return the actual state rather than claiming completion.
    let value = gh_json(
        vec![
            "pr".into(),
            "view".into(),
            request.number.to_string(),
            "--repo".into(),
            request.repository.clone(),
            "--json".into(),
            "number,url,state,headRefOid".into(),
        ],
        &request.working_directory,
    )?;
    let receipt = pr_receipt(&value, true)?;
    ensure!(
        receipt.head == request.expected_head,
        "head changed during merge; inspect GitHub before retrying"
    );
    Ok(receipt)
}
