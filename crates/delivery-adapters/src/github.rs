//! Collect and decode GitHub observations without making a merge decision.
use crate::{
    host::capture_text,
    protocol::{MergeRequest, MergeSnapshot, PullRequestReceipt, ReviewRecord},
    validation::validate_repository,
};
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::path::Path;

pub(super) fn gh_json(args: Vec<String>, cwd: &Path) -> Result<Value> {
    serde_json::from_str(&capture_text("gh", &args, cwd)?).context("invalid GitHub JSON response")
}

pub(super) fn pr_receipt(value: &Value, reused: bool) -> Result<PullRequestReceipt> {
    let state = contracts::PullRequestState::from_value(&value["state"])
        .context("missing or unknown PR state")?;
    Ok(PullRequestReceipt {
        number: value["number"].as_u64().context("missing PR number")?,
        url: value["url"].as_str().context("missing PR URL")?.to_owned(),
        state: state.to_string(),
        head: value["headRefOid"]
            .as_str()
            .context("missing PR head")?
            .to_owned(),
        reused,
    })
}

pub(super) fn github_snapshot(request: &MergeRequest) -> Result<MergeSnapshot> {
    let (owner, name) = validate_repository(&request.repository)?;
    let query = "query($owner:String!,$name:String!,$number:Int!){repository(owner:$owner,name:$name){defaultBranchRef{name target{... on Commit{oid}}} pullRequest(number:$number){headRefOid baseRefName baseRefOid state isDraft author{login} mergeable mergeStateStatus reviewDecision commits(last:1){nodes{commit{oid statusCheckRollup{contexts(first:100){nodes{__typename ... on CheckRun{status conclusion} ... on StatusContext{state}} pageInfo{hasNextPage}}}}}} reviews(first:100){nodes{author{login} state commit{oid}} pageInfo{hasNextPage}} reviewThreads(first:100){nodes{isResolved} pageInfo{hasNextPage}}}}}";
    let value = gh_json(
        vec![
            "api".into(),
            "graphql".into(),
            "-f".into(),
            format!("query={query}"),
            "-f".into(),
            format!("owner={owner}"),
            "-f".into(),
            format!("name={name}"),
            "-F".into(),
            format!("number={}", request.number),
        ],
        &request.working_directory,
    )?;
    decode_snapshot(&value)
}

fn decode_snapshot(value: &Value) -> Result<MergeSnapshot> {
    ensure!(
        value.get("errors").is_none(),
        "GitHub returned GraphQL errors"
    );
    let pr = &value["data"]["repository"]["pullRequest"];
    let commit = &pr["commits"]["nodes"][0]["commit"];
    let contexts = &commit["statusCheckRollup"]["contexts"];
    let reviews = &pr["reviews"];
    let threads = &pr["reviewThreads"];
    let complete = [contexts, reviews, threads]
        .iter()
        .all(|v| v["pageInfo"]["hasNextPage"] == false && v["nodes"].is_array())
        && commit["oid"] == pr["headRefOid"];
    let read = |key: &str| -> Result<String> {
        Ok(pr[key]
            .as_str()
            .with_context(|| format!("missing PR field {key}"))?
            .to_owned())
    };
    Ok(MergeSnapshot {
        head: read("headRefOid")?,
        base_ref: read("baseRefName")?,
        base_commit: read("baseRefOid")?,
        default_branch: value["data"]["repository"]["defaultBranchRef"]["name"]
            .as_str()
            .context("missing repository default branch")?
            .into(),
        default_commit: value["data"]["repository"]["defaultBranchRef"]["target"]["oid"]
            .as_str()
            .context("missing repository default commit")?
            .into(),
        state: read("state")?,
        draft: pr["isDraft"].as_bool().context("missing draft state")?,
        author: pr["author"]["login"]
            .as_str()
            .context("missing author")?
            .into(),
        mergeable: read("mergeable")?,
        merge_state: read("mergeStateStatus")?,
        review_decision: match pr
            .get("reviewDecision")
            .context("missing review decision")?
        {
            Value::Null => None,
            Value::String(value) if !value.is_empty() => Some(value.clone()),
            _ => anyhow::bail!("invalid review decision"),
        },
        complete,
        checks: contexts["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|c| {
                if c["__typename"] == "CheckRun" {
                    if c["status"] == contracts::CheckState::Completed.as_str() {
                        c["conclusion"]
                            .as_str()
                            .unwrap_or(contracts::CheckConclusion::Unknown.as_str())
                            .into()
                    } else {
                        contracts::CheckConclusion::Pending.to_string()
                    }
                } else if c["__typename"] == "StatusContext" {
                    c["state"]
                        .as_str()
                        .unwrap_or(contracts::CheckConclusion::Unknown.as_str())
                        .into()
                } else {
                    contracts::CheckConclusion::Unknown.to_string()
                }
            })
            .collect(),
        reviews: reviews["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|r| ReviewRecord {
                author: r["author"]["login"].as_str().unwrap_or("").into(),
                state: r["state"]
                    .as_str()
                    .unwrap_or(contracts::ReviewState::Unknown.as_str())
                    .into(),
                commit: r["commit"]["oid"].as_str().unwrap_or("").into(),
            })
            .collect(),
        unresolved_threads: threads["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|t| t["isResolved"] != true)
            .count(),
    })
}

#[cfg(test)]
#[path = "github_tests.rs"]
mod tests;
