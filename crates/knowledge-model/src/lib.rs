//! Backend-neutral external knowledge contracts. Contents stay in the configured target.
use anyhow::{Result, ensure};
use collaboration_model::{Outcome, digest, id};
use contracts::{KnowledgeAccess, KnowledgeBackend};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Component, Path, PathBuf},
};

pub const MAX_DOCUMENT_BYTES: usize = 1024 * 1024;
pub const MAX_SNAPSHOT_BYTES: usize = 32 * 1024 * 1024;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub kind: KnowledgeBackend,
    pub path: PathBuf,
    pub content_root: PathBuf,
    pub enabled: bool,
    pub access: KnowledgeAccess,
    /// Required for Git; a prepared branch must not be checked out.
    pub reference: Option<String>,
    /// Optional configured Git remote name. URLs and secrets are never accepted here.
    pub remote: Option<String>,
    #[serde(default)]
    pub credential_env: Vec<String>,
}
impl Target {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.path.is_absolute(), "knowledge_absolute_path_required");
        relative(&self.content_root)?;
        ensure!(
            self.credential_env.len() <= 16,
            "knowledge_environment_limit"
        );
        for name in &self.credential_env {
            ensure!(
                !name.is_empty()
                    && name.len() <= 128
                    && name
                        .bytes()
                        .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
                    && !name.starts_with("GIT_CONFIG")
                    && !matches!(
                        name.as_str(),
                        "PATH"
                            | "HOME"
                            | "LD_PRELOAD"
                            | "LD_LIBRARY_PATH"
                            | "GIT_INDEX_FILE"
                            | "GIT_DIR"
                            | "GIT_WORK_TREE"
                    ),
                "knowledge_invalid_credential_reference"
            );
        }
        if let Some(remote) = &self.remote {
            id(remote)?;
        }
        match self.kind {
            KnowledgeBackend::Git => ensure!(
                self.reference
                    .as_ref()
                    .is_some_and(|r| r.starts_with("refs/heads/") && r.len() <= 256),
                "knowledge_branch_required"
            ),
            KnowledgeBackend::Directory => ensure!(
                self.reference.is_none() && self.remote.is_none() && self.credential_env.is_empty(),
                "knowledge_directory_git_options"
            ),
        }
        Ok(())
    }
    pub fn digest(&self) -> Result<String> {
        Ok(digest(&serde_json::to_vec(self)?))
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Configuration {
    pub revision: u64,
    pub targets: BTreeMap<String, Target>,
}
impl Configuration {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.targets.len() <= 32, "knowledge_target_limit");
        for (name, target) in &self.targets {
            id(name)?;
            target.validate()?;
        }
        Ok(())
    }
    pub fn publication_binding(&self, target: &str, outcome_digest: &str) -> Result<String> {
        Ok(digest(&serde_json::to_vec(&(
            self.binding(target)?,
            outcome_digest,
        ))?))
    }
    pub fn binding(&self, target: &str) -> Result<String> {
        let value = self
            .targets
            .get(target)
            .ok_or_else(|| anyhow::anyhow!("knowledge_target_not_found"))?;
        Ok(digest(&serde_json::to_vec(&(
            self.revision,
            target,
            value,
        ))?))
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    List,
    Save {
        configuration: Configuration,
    },
    Probe {
        target: String,
    },
    Read {
        target: String,
        path: PathBuf,
        revision: Option<String>,
    },
    Preview {
        target: String,
        outcome: Box<Outcome>,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Preview {
    pub target: String,
    pub configuration_revision: u64,
    /// Opaque baseline binds both external content and the exact target configuration.
    pub expected_revision: String,
    pub path: String,
    pub before: Option<String>,
    pub after: String,
    pub content_digest: String,
}
pub fn relative(path: &Path) -> Result<()> {
    ensure!(!path.as_os_str().is_empty() && path.to_str().is_some_and(|s| s.len() <= 1024 && !s.contains(['\0', '\n', '\r', '\\', ':', ','])), "knowledge_invalid_path");
    ensure!(
        path.components().all(|c| matches!(c, Component::Normal(_)))
            && !path
                .components()
                .any(|c| c.as_os_str() == ".git" || c.as_os_str() == ".outcomehub"),
        "knowledge_path_escape"
    );
    Ok(())
}
pub fn baseline(value: &str) -> &str {
    value
        .split_once('@')
        .map_or(value, |(revision, _)| revision)
}
pub fn validate_write(
    service: &str,
    write: &collaboration_model::KnowledgeWrite,
    outcome: &Outcome,
) -> Result<()> {
    outcome.validate_digest()?;
    id(&write.input.id)?;
    id(&write.input.outcome_id)?;
    id(&write.input.repository)?;
    ensure!(
        write.input.service_id == service
            && write.input.outcome_id == outcome.input.id
            && write.input.outcome_digest == outcome.digest
            && !write.input.acceptance_id.is_empty(),
        "knowledge_binding_mismatch"
    );
    ensure!(
        write.request_digest == digest(&serde_json::to_vec(&write.input)?),
        "knowledge_request_digest_mismatch"
    );
    ensure!(
        (1..=600).contains(&write.input.max_seconds)
            && write.deadline_ms > collaboration_model::now_ms(),
        "knowledge_deadline_expired"
    );
    Ok(())
}
pub fn result(
    write: &collaboration_model::KnowledgeWrite,
    state: contracts::KnowledgeWriteState,
    reference: String,
    revision: String,
    summary: &str,
) -> collaboration_model::KnowledgeWriteResult {
    collaboration_model::KnowledgeWriteResult {
        id: write.input.id.clone(),
        request_digest: write.request_digest.clone(),
        state,
        reference,
        revision,
        summary: summary.into(),
    }
}
