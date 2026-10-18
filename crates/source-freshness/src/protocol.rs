//! Retained host evidence records with stable serialized fields.
use crate::identity::{trusted_repository, validate_branch, validate_hex};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RemoteSnapshot {
    pub repository: String,
    pub default_branch: String,
    pub commit: String,
    pub checked_at: String,
}

impl RemoteSnapshot {
    pub fn validate(&self) -> Result<()> {
        trusted_repository(&self.repository)?;
        validate_branch(&self.default_branch)?;
        validate_hex(&self.commit, 40)?;
        chrono::DateTime::parse_from_rfc3339(&self.checked_at)
            .context("invalid freshness timestamp")?;
        Ok(())
    }
    pub fn same_binding(&self, other: &Self) -> bool {
        self.repository.eq_ignore_ascii_case(&other.repository)
            && self.default_branch == other.default_branch
            && self.commit == other.commit
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillSnapshot {
    pub name: String,
    pub repository: String,
    pub installed_path: PathBuf,
    pub runtime_path: PathBuf,
    pub tree_sha256: String,
    pub runtime_sha256: String,
    pub release_tag: Option<String>,
    pub release_id: Option<u64>,
    pub source_commit: Option<String>,
    /// A locally built current default-branch skill may supersede a release only
    /// with a host-retained, passing full gate on that exact source commit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_qualitygate: Option<SourceQualitygate>,
    pub checked_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceQualitygate {
    pub path: PathBuf,
    pub sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillsManifest {
    pub schema_version: u32,
    pub skills: Vec<SkillSnapshot>,
}

#[cfg(test)]
#[path = "protocol_tests.rs"]
mod tests;
