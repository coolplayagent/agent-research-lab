//! Host path authority checks; these do not replace OS-level isolation.
use crate::files::{read_strict, write_optional};
use anyhow::{Context, Result, ensure};
use delivery::VerificationArtifact;
use evolution::HoldoutIsolation;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

pub(super) fn prepare_skills_artifact(
    base: &Path,
    artifact: &mut VerificationArtifact,
    isolation: &HoldoutIsolation,
) -> Result<()> {
    artifact.path = relative_to(base, &artifact.path).canonicalize()?;
    ensure_host_path(&artifact.path, isolation)?;
    artifact.validate_skills()
}

pub(super) fn check_gate_paths(
    input: &Path,
    context: &Path,
    output: Option<&Path>,
    isolation: &HoldoutIsolation,
) -> Result<()> {
    isolation.validate()?;
    ensure_host_path(input, isolation)?;
    ensure_host_path(context, isolation)?;
    if let Some(output) = output {
        ensure_host_path(output, isolation)?;
        ensure!(
            output != input && output != resolve_destination(context)?,
            "receipt cannot replace its request or expected context"
        );
    }
    Ok(())
}

pub(super) fn ensure_host_path(path: &Path, isolation: &HoldoutIsolation) -> Result<()> {
    let resolved = resolve_destination(path)?;
    for exposed in isolation
        .optimizer_roots
        .iter()
        .chain(&isolation.shared_knowledge_roots)
        .chain(&isolation.shared_memory_roots)
    {
        let exposed = exposed.canonicalize()?;
        ensure!(
            !resolved.starts_with(exposed),
            "gate requests, expected context and receipts must remain outside agent workspaces and shared stores"
        );
    }
    Ok(())
}

pub(super) fn relative_to(base: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    }
}

pub(super) fn resolve_isolation(base: &Path, isolation: &mut HoldoutIsolation) {
    isolation.private_root = relative_to(base, &isolation.private_root);
    for path in isolation
        .optimizer_roots
        .iter_mut()
        .chain(&mut isolation.shared_knowledge_roots)
        .chain(&mut isolation.shared_memory_roots)
    {
        *path = relative_to(base, path);
    }
}

pub(super) fn resolve_destination(path: &Path) -> Result<PathBuf> {
    if path.exists() {
        ensure!(
            !path.symlink_metadata()?.file_type().is_symlink(),
            "output or host evidence path cannot be a symlink"
        );
        return Ok(path.canonicalize()?);
    }
    let filename = path.file_name().context("path needs a filename")?;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    Ok(parent
        .canonicalize()
        .with_context(|| format!("parent directory must exist: {}", parent.display()))?
        .join(filename))
}

pub(super) fn validate_holdout(
    input: &Path,
    base: &Path,
    output: Option<PathBuf>,
) -> Result<Value> {
    let mut isolation: HoldoutIsolation = read_strict(input)?;
    resolve_isolation(base, &mut isolation);
    isolation.validate()?;
    let result =
        json!({"paths_valid":true,"os_isolation_verified":false,"holdout_isolation":isolation});
    write_optional(output.as_deref(), &result)?;
    Ok(result)
}

#[cfg(test)]
#[path = "paths_tests.rs"]
mod tests;
