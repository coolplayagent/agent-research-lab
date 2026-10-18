//! Holdout path separation checks; host sandbox permissions remain required.
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

/// All paths must exist. Canonicalization detects symlinks that would otherwise
/// leak held-out tasks through a seemingly separate workspace or shared store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HoldoutIsolation {
    pub private_root: PathBuf,
    pub optimizer_roots: Vec<PathBuf>,
    pub shared_knowledge_roots: Vec<PathBuf>,
    pub shared_memory_roots: Vec<PathBuf>,
}

impl HoldoutIsolation {
    pub fn validate(&self) -> Result<()> {
        let private = canonical_directory(&self.private_root)?;
        ensure!(
            !self.optimizer_roots.is_empty(),
            "optimizer must have an explicit workspace"
        );
        for path in self
            .optimizer_roots
            .iter()
            .chain(&self.shared_knowledge_roots)
            .chain(&self.shared_memory_roots)
        {
            let exposed = canonical_directory(path)?;
            ensure!(
                !private.starts_with(&exposed) && !exposed.starts_with(&private),
                "holdout directory overlaps an agent workspace or shared store"
            );
            // Inspect nested symlinks as well; the root itself can be safe while
            // a descendant exposes the protected tree.
            reject_holdout_links(&exposed, &private)?;
        }
        Ok(())
    }

    pub fn authorize_optimizer_path(&self, path: &Path) -> Result<()> {
        self.validate()?;
        let resolved = path
            .canonicalize()
            .with_context(|| format!("cannot resolve {}", path.display()))?;
        let private = self.private_root.canonicalize()?;
        ensure!(
            !resolved.starts_with(&private),
            "optimizer cannot access held-out tasks or answers"
        );
        for root in &self.optimizer_roots {
            if resolved.starts_with(root.canonicalize()?) {
                return Ok(());
            }
        }
        bail!("optimizer path is outside its declared workspaces")
    }
}

fn canonical_directory(path: &Path) -> Result<PathBuf> {
    let canonical = path
        .canonicalize()
        .with_context(|| format!("cannot resolve {}", path.display()))?;
    ensure!(canonical.is_dir(), "{} must be a directory", path.display());
    Ok(canonical)
}

fn reject_holdout_links(exposed: &Path, private: &Path) -> Result<()> {
    let mut pending = vec![exposed.to_path_buf()];
    let mut visited = BTreeSet::new();
    while let Some(directory) = pending.pop() {
        if !visited.insert(directory.clone()) {
            continue;
        }
        for entry in std::fs::read_dir(&directory)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if file_type.is_symlink() {
                let target = entry
                    .path()
                    .canonicalize()
                    .context("unresolvable symlink in exposed workspace")?;
                ensure!(
                    !target.starts_with(private) && !private.starts_with(&target),
                    "nested symlink exposes held-out tasks"
                );
                // Directory links outside the declared roots may lead back to
                // the private tree. Reject them rather than traversing the host.
                if target.is_dir() {
                    ensure!(
                        target.starts_with(exposed),
                        "directory symlink escapes declared workspace"
                    );
                    pending.push(target);
                }
            } else if file_type.is_dir() {
                pending.push(entry.path());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "holdout_tests.rs"]
mod tests;
