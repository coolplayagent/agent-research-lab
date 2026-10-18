//! Public proposal records and syntactic validation; references are never fetched.
use crate::limits::MAX_RECIPIENTS;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    path::{Component, Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostMember {
    pub cohort_id: String,
    pub task_id: String,
    pub run_id: String,
    /// Digest of the host-frozen launch/job binding, never supplied by an agent.
    pub authority_sha256: String,
}

impl HostMember {
    pub(super) fn validate(&self) -> Result<()> {
        digest_id(&self.cohort_id)?;
        digest_id(&self.authority_sha256)?;
        config::safe_id(&self.task_id)?;
        ensure!(
            !self.run_id.is_empty()
                && self.run_id.len() <= 128
                && self
                    .run_id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
            "run identity must be 1..128 safe ASCII characters"
        );
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Finding,
    Question,
    Counterexample,
    Reference,
}

/// Syntactic references only: the board never reads these paths or fetches URLs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reference {
    pub repository: String,
    pub commit: String,
    pub path: String,
    pub sha256: String,
    pub start_line: u32,
    pub end_line: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Proposal {
    pub schema_version: u32,
    pub id: String,
    pub kind: Kind,
    pub text: String,
    pub topics: Vec<String>,
    pub references: Vec<Reference>,
    pub reply_to: Option<String>,
    /// Empty means public. Otherwise sorted, distinct host-registered task IDs.
    /// Omission for public messages preserves existing v1 evidence digests.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recipients: Vec<String>,
}

impl Proposal {
    pub(super) fn validate(&self) -> Result<()> {
        ensure!(self.schema_version == 1, "unsupported proposal schema");
        config::safe_id(&self.id)?;
        ensure!(
            !self.text.trim().is_empty() && self.text.len() <= 1024,
            "invalid proposal text size"
        );
        topics(&self.topics, 4)?;
        ensure!(
            self.recipients.len() <= MAX_RECIPIENTS,
            "too many recipients"
        );
        for recipient in &self.recipients {
            config::safe_id(recipient)?;
        }
        ensure!(
            self.recipients.windows(2).all(|pair| pair[0] < pair[1]),
            "recipients must be sorted and distinct"
        );
        ensure!(self.references.len() <= 4, "too many references");
        for r in &self.references {
            ensure!(
                !r.repository.is_empty()
                    && r.repository.len() <= 100
                    && r.repository
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"/_-.".contains(&b)),
                "invalid reference repository"
            );
            ensure!(
                r.commit.len() == 40 && hex(&r.commit),
                "invalid reference commit"
            );
            digest_id(&r.sha256)?;
            ensure!(
                !r.path.is_empty()
                    && r.path.len() <= 256
                    && !Path::new(&r.path).is_absolute()
                    && Path::new(&r.path)
                        .components()
                        .all(|c| matches!(c, Component::Normal(_))),
                "invalid reference path"
            );
            ensure!(
                r.start_line > 0 && r.end_line >= r.start_line && r.end_line - r.start_line <= 200,
                "invalid reference lines"
            );
        }
        if let Some(id) = &self.reply_to {
            digest_id(id)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Endpoint {
    pub outbox: PathBuf,
    pub view: PathBuf,
}

/// Host-selected subscriptions and/or an explicit bounded search. Empty means no context.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    pub topics: Vec<String>,
    pub query: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSnapshot {
    pub schema_version: u32,
    pub cohort_id: String,
    pub revision: u64,
    pub as_of: u64,
    pub message_ids: Vec<String>,
    pub truncated: bool,
    pub text: String,
    pub digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Message {
    pub id: String,
    pub task_id: String,
    pub run_id: String,
    pub authority_sha256: String,
    pub slot: usize,
    pub created_at: u64,
    pub expires_at: u64,
    pub reply_depth: u8,
    pub proposal_sha256: String,
    pub proposal: Proposal,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PollReport {
    pub scanned_slots: usize,
    pub accepted: usize,
    pub duplicates: usize,
    pub rejected: usize,
    pub changed_consumed_slots: usize,
    pub cohort_accepted_total: usize,
    pub expired_messages: usize,
    pub cohort_capacity_exhausted: bool,
    pub run_quota_exhausted: Vec<String>,
    pub task_quota_exhausted: Vec<String>,
    pub revision: u64,
}

/// Saved in private Launch authority before spawning; the worker cannot replace
/// the context digest with a later board view or a rewritten log file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaunchContext {
    pub member: HostMember,
    pub context: ContextSnapshot,
}

fn hex(s: &str) -> bool {
    s.bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub(super) fn digest_id(s: &str) -> Result<()> {
    ensure!(
        s.len() == 64 && hex(s),
        "expected lowercase SHA-256 identity"
    );
    Ok(())
}

pub(super) fn topics(values: &[String], max: usize) -> Result<()> {
    ensure!(values.len() <= max, "too many topics");
    let mut seen = BTreeSet::new();
    for topic in values {
        config::safe_id(topic)?;
        ensure!(
            topic.len() <= 32 && seen.insert(topic),
            "invalid or repeated topic"
        );
    }
    Ok(())
}

#[cfg(test)]
#[path = "protocol_tests.rs"]
mod tests;
