//! Existing delivery request, receipt and remote-observation wire records.
use crate::evidence::ReleaseEvidence;
use contracts::InstallationPhase;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InstallRequest {
    pub skill_source: PathBuf,
    pub skills_root: PathBuf,
    /// Outside skills_root, on the same filesystem so directory renames are atomic.
    pub backup_root: PathBuf,
    pub skill_directory_name: String,
    pub binary_relative_path: PathBuf,
    pub binary_sha256: String,
    pub version: String,
    pub evidence: ReleaseEvidence,
    pub smoke_arguments: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InstallReceipt {
    pub operation_id: String,
    pub target: PathBuf,
    pub backup: Option<PathBuf>,
    pub previous_sha256: Option<String>,
    pub package_sha256: String,
    pub binary_sha256: String,
    pub binary_relative_path: PathBuf,
    pub version: String,
    pub evidence: ReleaseEvidence,
    pub phase: InstallationPhase,
    pub receipt_path: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PullRequestRequest {
    pub repository: String,
    pub working_directory: PathBuf,
    pub branch: String,
    pub base: String,
    pub expected_head: String,
    pub title: String,
    pub body: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PullRequestReceipt {
    pub number: u64,
    pub url: String,
    pub state: String,
    pub head: String,
    pub reused: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MergeRequest {
    pub repository: String,
    pub working_directory: PathBuf,
    pub number: u64,
    pub expected_head: String,
    pub evidence: ReleaseEvidence,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReviewRecord {
    pub author: String,
    pub state: String,
    pub commit: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MergeSnapshot {
    pub head: String,
    pub base_ref: String,
    pub base_commit: String,
    pub default_branch: String,
    pub default_commit: String,
    pub state: String,
    pub draft: bool,
    pub author: String,
    pub mergeable: String,
    pub merge_state: String,
    pub review_decision: Option<String>,
    pub checks: Vec<String>,
    pub reviews: Vec<ReviewRecord>,
    pub unresolved_threads: usize,
    pub complete: bool,
}

#[cfg(test)]
#[path = "protocol_tests.rs"]
mod tests;
