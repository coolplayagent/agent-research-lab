//! One bounded index update, with explicit completeness and active-work handling.
use crate::{
    host::relay,
    protocol::{IndexRefreshRequest, IndexRefreshStatus},
};
use anyhow::Result;
use delivery::{validate_commit, validate_identifier};
use serde_json::Value;

fn snapshot(value: &Value) -> &Value {
    value
        .get("status")
        .filter(|v| v.is_object())
        .unwrap_or(value)
}
fn ready(value: &Value, expected: &str) -> bool {
    let value = snapshot(value);
    let integrity = &value["content_integrity"];
    // The installed 1.1.18 contract uses state="complete". Retain explicit
    // Boolean compatibility without treating a missing integrity report as proof.
    let complete = match integrity.get("state") {
        Some(state) => {
            contracts::ContentIntegrityState::from_value(state)
                == Some(contracts::ContentIntegrityState::Complete)
        }
        None => integrity.get("complete") == Some(&Value::Bool(true)),
    };
    value["last_indexed_commit"].as_str() == Some(expected) && value["stale"] == false && complete
}

/// Submit at most one update. Queued/running tasks are returned for the workflow to poll later.
pub fn refresh_index(request: &IndexRefreshRequest) -> Result<IndexRefreshStatus> {
    validate_commit(&request.expected_commit)?;
    validate_identifier(&request.repository_alias)?;
    let mut status = relay(
        &request.relay_knowledge_binary,
        &request.knowledge_root,
        &[
            "repo",
            "status",
            &request.repository_alias,
            "--format",
            "json",
        ],
    )?;
    let snapshot_status = snapshot(&status);
    let active = status.get("active_task").is_some_and(|v| !v.is_null())
        || snapshot_status
            .get("active_task")
            .is_some_and(|v| !v.is_null());
    let maintenance_pending = status["maintenance_pending"] == true
        || status["retention"]["maintenance_pending"] == true
        || snapshot_status["maintenance_pending"] == true
        || snapshot_status["retention"]["maintenance_pending"] == true;
    if !ready(&status, &request.expected_commit) && !active && !maintenance_pending {
        relay(
            &request.relay_knowledge_binary,
            &request.knowledge_root,
            &[
                "repo",
                "update",
                &request.repository_alias,
                "--head",
                &request.expected_commit,
                "--format",
                "json",
            ],
        )?;
        status = relay(
            &request.relay_knowledge_binary,
            &request.knowledge_root,
            &[
                "repo",
                "status",
                &request.repository_alias,
                "--format",
                "json",
            ],
        )?;
    }
    Ok(IndexRefreshStatus {
        expected_commit: request.expected_commit.clone(),
        ready: ready(&status, &request.expected_commit),
        status,
    })
}

#[cfg(test)]
#[path = "index_refresh_tests.rs"]
mod tests;
