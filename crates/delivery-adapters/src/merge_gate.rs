//! Decide merge eligibility from bound release evidence and a complete remote snapshot.
use crate::protocol::{MergeRequest, MergeSnapshot};
use anyhow::{Context, Result, ensure};
use std::collections::BTreeMap;

/// Missing, stale, queued, skipped, truncated and unknown evidence is never a pass.
pub fn evaluate_merge_gate(request: &MergeRequest, snapshot: &MergeSnapshot) -> Result<()> {
    request.evidence.validate()?;
    let upstream = request
        .evidence
        .upstream
        .as_ref()
        .context("missing release upstream baseline")?;
    ensure!(
        request.repository == upstream.repository,
        "release evidence belongs to a different repository"
    );
    ensure!(
        snapshot.base_ref == upstream.default_branch
            && snapshot.default_branch == upstream.default_branch
            && snapshot.base_commit == upstream.commit
            && snapshot.default_commit == upstream.commit,
        "upstream default branch advanced or PR targets another base; rebase and re-evaluate before merging"
    );
    ensure!(
        request.expected_head == request.evidence.source_commit
            && snapshot.head == request.expected_head,
        "PR head does not match evaluated source"
    );
    ensure!(snapshot.complete, "GitHub evidence is missing or truncated");
    ensure!(
        snapshot.state == contracts::PullRequestState::Open.as_str() && !snapshot.draft,
        "PR must be open and ready"
    );
    ensure!(
        snapshot.mergeable == contracts::MergeabilityState::Mergeable.as_str()
            && snapshot.merge_state == contracts::MergeQueueState::Clean.as_str(),
        "GitHub merge requirements are not satisfied"
    );
    ensure!(
        !snapshot.checks.is_empty()
            && snapshot
                .checks
                .iter()
                .all(|check| check == contracts::CheckConclusion::Success.as_str()),
        "all CI checks must have succeeded"
    );
    ensure!(
        snapshot.unresolved_threads == 0,
        "unresolved review threads remain"
    );
    ensure!(
        snapshot
            .review_decision
            .as_deref()
            .is_none_or(|value| value == contracts::ReviewState::Approved.as_str()),
        "required GitHub approvals are missing"
    );
    let mut latest = BTreeMap::new();
    for review in &snapshot.reviews {
        if review.state == contracts::ReviewState::Approved.as_str()
            || review.state == contracts::ReviewState::ChangesRequested.as_str()
            || review.state == contracts::ReviewState::Dismissed.as_str()
        {
            latest.insert(&review.author, review);
        }
    }
    ensure!(
        !latest
            .values()
            .any(|r| r.state == contracts::ReviewState::ChangesRequested.as_str()),
        "changes have been requested"
    );
    // reviewDecision is GitHub's aggregate of the repository's required approval policy.
    // Repositories without required reviewers expose null; independent evaluation remains mandatory.
    Ok(())
}

#[cfg(test)]
#[path = "merge_gate_tests.rs"]
mod tests;
