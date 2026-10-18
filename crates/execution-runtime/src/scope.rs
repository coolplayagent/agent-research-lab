//! Validated host-selected task boundaries and scoped run projection.
use crate::job_store::job_path;
use crate::status::status;
use anyhow::{Context, Result, ensure};
use config::{Config, safe_id};
use native_effects::fs;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use task::Job;

/// Host-selected IDs are frozen once before execution. This is an admission
/// boundary, not an override for global orphan recovery or effect reconciliation.
pub(super) struct TaskScope {
    pub(super) ids: BTreeSet<String>,
    pub(super) sha256: String,
}

impl TaskScope {
    pub(super) fn load(c: &Config, path: &Path) -> Result<Self> {
        use std::{io::Read, os::unix::fs::OpenOptionsExt, path::Component};
        ensure!(
            path.is_absolute() && path.starts_with(c.state_dir.join("private")),
            "task ID file must be under state_dir/private"
        );
        let mut ancestor = PathBuf::new();
        for part in path.components() {
            ensure!(
                !matches!(part, Component::CurDir | Component::ParentDir),
                "noncanonical task ID file path"
            );
            ancestor.push(part);
            ensure!(
                !fs::symlink_metadata(&ancestor)?.file_type().is_symlink(),
                "task ID file traverses a symlink"
            );
        }
        let mut file = fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(path)?;
        let metadata = file.metadata()?;
        ensure!(
            metadata.is_file() && metadata.len() <= 64 * 1024,
            "task ID file must be a regular file of at most 64 KiB"
        );
        let mut bytes = Vec::new();
        file.by_ref().take(64 * 1024 + 1).read_to_end(&mut bytes)?;
        ensure!(bytes.len() <= 64 * 1024, "task ID file exceeds 64 KiB");
        let requested: Vec<String> = serde_json::from_slice(&bytes)
            .context("task ID file must contain a JSON string array")?;
        ensure!(
            !requested.is_empty() && requested.len() <= 256,
            "task scope requires 1..256 IDs"
        );
        let mut ids = BTreeSet::new();
        for id in requested {
            safe_id(&id)?;
            ensure!(ids.insert(id), "duplicate task ID in scope");
        }
        let config_digest = storage::digest(&serde_json::to_vec(c)?);
        for id in &ids {
            let job: Job = storage::read(&job_path(c, id))
                .with_context(|| format!("selected task is not enqueued: {id}"))?;
            ensure!(job.task.id == *id, "selected job identity mismatch: {id}");
            ensure!(
                job.config_digest == config_digest,
                "selected task configuration differs from this run: {id}"
            );
            ensure!(
                job.task
                    .dependencies
                    .iter()
                    .all(|dependency| ids.contains(dependency)),
                "selected task has an out-of-scope dependency: {id}"
            );
        }
        Ok(Self {
            ids,
            sha256: storage::digest(&bytes),
        })
    }
}

pub(super) fn scoped_run_status(
    c: &Config,
    scope: Option<&TaskScope>,
    deferred: &BTreeSet<String>,
) -> Result<Value> {
    let mut result = status(c)?;
    if let Some(scope) = scope {
        result["task_scope"] = json!({"task_ids":scope.ids,"file_sha256":scope.sha256,"deferred_followup_runs":deferred,"automatic_seed":false,"automation_outbox":false});
    }
    Ok(result)
}

#[cfg(test)]
#[path = "scope_tests.rs"]
mod tests;
