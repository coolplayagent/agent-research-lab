//! Immutable private experiment files; no publication or external knowledge writes.
use crate::protocol::Plan;
use anyhow::{Context, Result, ensure};
use config::Config;
use serde::Serialize;
use serde_json::Value;
use std::{
    fs,
    path::{Component, Path, PathBuf},
};

pub(super) fn private_path(c: &Config, path: &Path, create_parent: bool) -> Result<()> {
    ensure!(
        path.is_absolute() && path.starts_with(c.state_dir.join("private")),
        "experiment artifacts must be under state_dir/private"
    );
    let mut current = PathBuf::new();
    for component in path
        .parent()
        .context("artifact has no parent")?
        .components()
    {
        ensure!(
            !matches!(component, Component::ParentDir | Component::CurDir),
            "noncanonical artifact path"
        );
        current.push(component);
        if !current.exists() && create_parent {
            fs::create_dir(&current)?;
        }
        let m = fs::symlink_metadata(&current)?;
        ensure!(
            m.is_dir() && !m.file_type().is_symlink(),
            "artifact path traverses a symlink"
        );
    }
    ensure!(
        path.file_name().is_some_and(|n| n != ".."),
        "invalid artifact name"
    );
    if path.exists() {
        ensure!(
            !fs::symlink_metadata(path)?.file_type().is_symlink(),
            "artifact is a symlink"
        );
    }
    Ok(())
}

pub(super) fn immutable_write(c: &Config, path: &Path, value: &impl Serialize) -> Result<()> {
    private_path(c, path, true)?;
    if path.exists() {
        ensure!(
            storage::read::<Value>(path)? == serde_json::to_value(value)?,
            "immutable experiment artifact already differs"
        );
    } else {
        storage::write(path, value)?;
    }
    Ok(())
}

pub(super) fn materialize(c: &Config, root: &Path, plan: &Plan) -> Result<()> {
    immutable_write(c, &root.join("manifest.json"), plan)?;
    for entry in &plan.tasks {
        immutable_write(
            c,
            &root.join("tasks").join(format!("{}.json", entry.task.id)),
            &entry.task,
        )?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "artifacts_tests.rs"]
mod tests;
