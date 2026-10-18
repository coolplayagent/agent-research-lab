//! Offline conversion. The source is immutable, and only a fully verified copy is published.
mod database;
mod records;
mod snapshot;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use snapshot::{Offline, inventory};
use std::{collections::BTreeMap, fs, path::Path};
const MANIFEST: &str = "outcome-hub-migration.json";
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub format: u32,
    pub tool_version: String,
    pub source_sha256: String,
    pub target_sha256: String,
    pub source_files: BTreeMap<String, String>,
    pub target_files: BTreeMap<String, String>,
    pub converted: Vec<String>,
}
fn identity(files: &BTreeMap<String, String>) -> Result<String> {
    Ok(storage::digest(&serde_json::to_vec(files)?))
}
pub fn inspect(source: &Path) -> Result<BTreeMap<String, String>> {
    let _offline = Offline::open(source)?;
    let files = inventory(source)?;
    ensure!(!files.contains_key(MANIFEST), "source is already migrated");
    // Exercise exactly the conversion and validation path in a disposable copy.
    let temp = tempfile::tempdir()?;
    snapshot::copy(source, temp.path(), &files)?;
    convert_copy(temp.path(), &files)?;
    ensure!(
        inventory(source)? == files,
        "source changed during inspection"
    );
    Ok(files)
}
fn convert_copy(root: &Path, files: &BTreeMap<String, String>) -> Result<Vec<String>> {
    let mut changed = Vec::new();
    for name in files.keys().filter(|name| {
        Path::new(name)
            .file_name()
            .is_some_and(|n| n == "messages.sqlite")
    }) {
        database::convert(&root.join(name))?;
        changed.push(name.clone());
    }
    records::convert(root, files, &mut changed)?;
    Ok(changed)
}
pub fn convert(source: &Path, destination: &Path) -> Result<Manifest> {
    let _offline = Offline::open(source)?;
    ensure!(!destination.try_exists()?, "destination already exists");
    let source_path = source.canonicalize()?;
    let parent = destination
        .parent()
        .context("destination requires a parent")?
        .canonicalize()?;
    ensure!(
        !parent.starts_with(&source_path),
        "destination must be outside source"
    );
    let files = inventory(source)?;
    ensure!(!files.contains_key(MANIFEST), "source is already migrated");
    let temp = tempfile::Builder::new()
        .prefix(".outcome-hub-migrate-")
        .tempdir_in(&parent)?;
    snapshot::copy(source, temp.path(), &files)?;
    let converted = convert_copy(temp.path(), &files)?;
    ensure!(
        inventory(source)? == files,
        "source changed during migration"
    );
    let target_files = inventory(temp.path())?;
    let manifest = Manifest {
        format: 1,
        tool_version: env!("CARGO_PKG_VERSION").into(),
        source_sha256: identity(&files)?,
        target_sha256: identity(&target_files)?,
        source_files: files,
        target_files,
        converted,
    };
    storage::write(&temp.path().join(MANIFEST), &manifest)?;
    verify_contents(source, temp.path())?;
    snapshot::sync_tree(temp.path())?;
    snapshot::publish(temp.path(), destination)?;
    fs::File::open(parent)?.sync_all()?;
    Ok(manifest)
}
fn verify_contents(source: &Path, destination: &Path) -> Result<Manifest> {
    let manifest: Manifest = storage::read(&destination.join(MANIFEST))?;
    ensure!(
        manifest.format == 1 && manifest.tool_version == env!("CARGO_PKG_VERSION"),
        "unsupported migration manifest"
    );
    let original = inventory(source)?;
    let mut target = inventory(destination)?;
    target.remove(MANIFEST);
    ensure!(
        original == manifest.source_files && identity(&original)? == manifest.source_sha256,
        "source digest mismatch"
    );
    ensure!(
        target == manifest.target_files && identity(&target)? == manifest.target_sha256,
        "target digest mismatch"
    );
    records::validate(destination, &target)?;
    for name in target.keys().filter(|name| {
        Path::new(name)
            .file_name()
            .is_some_and(|n| n == "messages.sqlite")
    }) {
        database::validate(&destination.join(name))?;
    }
    Ok(manifest)
}
pub fn verify(source: &Path, destination: &Path) -> Result<Manifest> {
    let _source_offline = Offline::open(source)?;
    let _destination_offline = Offline::open(destination)?;
    verify_contents(source, destination)
}
#[cfg(test)]
mod tests;
