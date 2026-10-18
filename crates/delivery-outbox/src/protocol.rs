//! Stable persisted operations and queue records.
pub use contracts::QueueState;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", content = "request", rename_all = "snake_case")]
pub enum Operation {
    Install(delivery::InstallRequest),
    Pr(delivery::PullRequestRequest),
    Merge(delivery::MergeRequest),
    KnowledgePrepare(knowledge::PrepareReportRequest),
    KnowledgePublish(knowledge::PublishReportRequest),
    KnowledgeRefresh(knowledge::IndexRefreshRequest),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QueueEntry {
    pub id: String,
    pub state: QueueState,
    pub attempts: u32,
    pub request_path: PathBuf,
    pub result: Option<Value>,
    pub error: Option<String>,
    pub updated_at: String,
    pub retry_after_unix: i64,
}

#[cfg(test)]
#[path = "protocol_tests.rs"]
mod tests;
