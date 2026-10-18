//! Public preparation API and input-bound performance markers; never gate evidence.
use crate::report::SeedReport;
use crate::{
    bazel, cargo,
    copying::Seeder,
    files::{checked_directory, read_bounded},
};
use anyhow::Result;
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    time::Instant,
};

pub fn applicable(worktree: &Path) -> bool {
    worktree.join("Cargo.lock").is_file() || worktree.join("MODULE.bazel").is_file()
}

fn inputs_digest(worktree: &Path) -> Result<String> {
    let mut digest = Sha256::new();
    for name in [
        "Cargo.lock",
        "MODULE.bazel",
        "MODULE.bazel.lock",
        ".bazelversion",
        "rust-toolchain.toml",
    ] {
        digest.update(name);
        let path = worktree.join(name);
        if path.exists() {
            digest.update(read_bounded(&path, 8 * 1024 * 1024)?);
        }
    }
    Ok(format!("{:x}", digest.finalize()))
}

/// A preparation marker only avoids repeated copies; build tools still check
/// their lockfiles and archives. It never authorizes a gate or an agent effect.
pub fn ready(worktree: &Path, logs: &Path) -> Result<bool> {
    let marker = logs.join("build-cache-seed.json");
    if !marker.exists() {
        return Ok(false);
    }
    let report: SeedReport = serde_json::from_slice(&read_bounded(&marker, 1024 * 1024)?)?;
    Ok(report.schema_version == 1 && report.inputs_digest == inputs_digest(worktree)?)
}

/// Run before effect_claim, with no active workers, inside the host's accounted
/// prerequisite allowance. A deadline leaves resumable private cache files.
pub fn prepare(worktree: &Path, logs: &Path) -> Result<SeedReport> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let cargo = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| home.as_ref().map(|p| p.join(".cargo")));
    let cache = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| home.as_ref().map(|p| p.join(".cache")));
    prepare_from(worktree, logs, cargo.as_deref(), cache.as_deref())
}

pub(super) fn prepare_from(
    worktree: &Path,
    logs: &Path,
    cargo: Option<&Path>,
    cache: Option<&Path>,
) -> Result<SeedReport> {
    checked_directory(worktree, false)?;
    checked_directory(logs, true)?;
    let started = Instant::now();
    let mut seed = Seeder::new(
        started,
        SeedReport {
            schema_version: 1,
            inputs_digest: inputs_digest(worktree)?,
            ..SeedReport::default()
        },
    );
    if let Some(cargo) = cargo.filter(|p| p.is_dir()) {
        cargo::seed(&mut seed, worktree, logs, cargo)?;
    }
    if let Some(cache) = cache.filter(|p| p.is_dir()) {
        bazel::seed(&mut seed, worktree, logs, cache)?;
    }
    let report = seed.finish();
    storage::write(&logs.join("build-cache-seed.json"), &report)?;
    Ok(report)
}

#[cfg(test)]
#[path = "preparation_tests.rs"]
mod tests;
