//! Publication layout and request/receipt data; loading records performs no effects.
use crate::report::OutcomeReport;
use anyhow::{Result, ensure};
use delivery::{relative_path, validate_identifier};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationLayout {
    pub report_directory: PathBuf,
    pub navigation_files: Vec<PathBuf>,
    pub generated_roots: Vec<PathBuf>,
    pub topic: String,
    pub repository: String,
    pub expected_origin: String,
}

impl PublicationLayout {
    pub(super) fn validate(&self) -> Result<()> {
        relative_path(&self.report_directory)?;
        validate_identifier(&self.topic)?;
        ensure!(
            self.repository.split('/').count() == 2
                && self
                    .repository
                    .split('/')
                    .all(|v| validate_identifier(v).is_ok()),
            "invalid publication repository"
        );
        ensure!(
            !self.expected_origin.is_empty() && self.expected_origin.len() <= 2048,
            "explicit publication origin required"
        );
        ensure!(
            self.navigation_files.len() <= 32 && self.generated_roots.len() <= 16,
            "publication layout exceeds bound"
        );
        for path in self.navigation_files.iter().chain(&self.generated_roots) {
            relative_path(path)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PrepareReportRequest {
    pub layout: PublicationLayout,
    #[serde(rename = "superpod_root", alias = "knowledge_root")]
    pub knowledge_root: PathBuf,
    pub worktree: PathBuf,
    pub relay_knowledge_binary: PathBuf,
    pub report: OutcomeReport,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PreparedReport {
    pub layout: PublicationLayout,
    pub worktree: PathBuf,
    pub branch: String,
    pub baseline: String,
    pub report_path: PathBuf,
    pub report_sha256: String,
    pub experiment_id: String,
    pub relay_knowledge_binary: PathBuf,
    /// Every reviewed contribution path, including generated navigation/maps and deletions.
    pub publication_files: BTreeMap<PathBuf, Option<String>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PublishReportRequest {
    pub prepared: PreparedReport,
    pub base_branch: String,
    pub title: String,
    pub body: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IndexRefreshRequest {
    pub relay_knowledge_binary: PathBuf,
    #[serde(rename = "superpod_root", alias = "knowledge_root")]
    pub knowledge_root: PathBuf,
    pub repository_alias: String,
    pub expected_commit: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IndexRefreshStatus {
    pub expected_commit: String,
    pub ready: bool,
    pub status: Value,
}
