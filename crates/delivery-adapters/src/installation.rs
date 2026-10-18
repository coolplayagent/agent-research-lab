//! Journaled package installation and hash-checked recovery under one install lock.
use crate::{
    host::{capture_text, unique_id},
    package::{copy_package, file_sha256, skill_name, tree_sha256},
    protocol::{InstallReceipt, InstallRequest},
    validation::{relative_path, validate_digest, validate_identifier},
};
use anyhow::{Context, Result, bail, ensure};
use contracts::InstallationPhase;
use freshness::RemoteClient;
use serde_json::{Value, json};
use std::{
    fs::{self, File},
    io::Write,
    path::Path,
};

fn write_receipt(receipt: &InstallReceipt) -> Result<()> {
    let temporary = receipt.receipt_path.with_extension("tmp");
    let mut file = File::create(&temporary)?;
    file.write_all(&serde_json::to_vec_pretty(receipt)?)?;
    file.sync_all()?;
    fs::rename(temporary, &receipt.receipt_path)?;
    File::open(
        receipt
            .receipt_path
            .parent()
            .context("missing receipt parent")?,
    )?
    .sync_all()?;
    Ok(())
}

fn installation_lock(root: &Path, name: &str) -> Result<storage::LockGuard> {
    fs::create_dir_all(root)?;
    storage::lock(&root.join(format!("{name}.lock")))
        .context("another installation or rollback is active")
}

/// Activate a packaged skill and its bundled CLI together, then smoke-test that exact binary.
/// Installation must be called at a task boundary; active tasks retain their pinned versions.
pub fn install_candidate(request: &InstallRequest) -> Result<InstallReceipt> {
    install_candidate_with_client(request, &RemoteClient::default())
}

pub(super) fn install_candidate_with_client(
    request: &InstallRequest,
    remote: &RemoteClient,
) -> Result<InstallReceipt> {
    request.evidence.validate()?;
    let upstream = request
        .evidence
        .upstream
        .as_ref()
        .context("missing release upstream baseline")?;
    remote.verify_remote_binding(upstream)?;
    let skills_manifest = request
        .evidence
        .skills_manifest
        .as_ref()
        .context("missing release skills manifest")?;
    skills_manifest.verify_skills(remote)?;
    validate_identifier(&request.skill_directory_name)?;
    relative_path(&request.binary_relative_path)?;
    validate_digest(&request.binary_sha256)?;
    ensure!(
        !request.version.trim().is_empty() && !request.smoke_arguments.is_empty(),
        "version and smoke arguments are required"
    );
    let source = fs::canonicalize(&request.skill_source)?;
    ensure!(
        !fs::canonicalize(&skills_manifest.path)?.starts_with(&source),
        "skills manifest must be a host artifact outside the candidate package"
    );
    let name = skill_name(&source)?;
    let _lock = installation_lock(&request.backup_root, &request.skill_directory_name)?;
    fs::create_dir_all(&request.skills_root)?;
    let skills_root = fs::canonicalize(&request.skills_root)?;
    let backup_root = fs::canonicalize(&request.backup_root)?;
    ensure!(
        !backup_root.starts_with(&skills_root) && !skills_root.starts_with(&backup_root),
        "backups must be outside skill discovery"
    );
    ensure!(
        !source.starts_with(&skills_root),
        "candidate package must be staged outside installed skills"
    );
    let target = skills_root.join(&request.skill_directory_name);
    // A crash can occur between either rename and its journal update. Reconcile actual hashes.
    for entry in fs::read_dir(&backup_root)? {
        let path = entry?.path().join("receipt.json");
        if path.is_file() {
            let mut pending: InstallReceipt = serde_json::from_slice(&fs::read(&path)?)?;
            if pending.target == target
                && !matches!(
                    pending.phase,
                    InstallationPhase::Verified | InstallationPhase::RolledBack
                )
            {
                rollback_locked(&mut pending)
                    .context("recover unfinished installation before retrying")?;
            }
        }
    }
    if target.exists() {
        ensure!(
            !fs::symlink_metadata(&target)?.file_type().is_symlink(),
            "installed target cannot be a symlink"
        );
    }
    for entry in fs::read_dir(&skills_root)? {
        let path = entry?.path();
        if path != target && path.join("SKILL.md").is_file() {
            ensure!(
                skill_name(&path)? != name,
                "another installed directory already exposes this skill name: {}",
                path.display()
            );
        }
    }
    let source_hash = tree_sha256(&source)?;
    ensure!(
        file_sha256(&source.join(&request.binary_relative_path))? == request.binary_sha256,
        "binary digest mismatch"
    );
    let snapshot = request.evidence.snapshot_receipt.read()?;
    ensure!(
        snapshot["package_sha256"].as_str() == Some(&source_hash)
            && snapshot["binary_sha256"].as_str() == Some(&request.binary_sha256),
        "package is not the artifact verified by the host"
    );
    if target.exists() {
        let active_hash = tree_sha256(&target)?;
        for entry in fs::read_dir(&backup_root)? {
            let receipt_path = entry?.path().join("receipt.json");
            if receipt_path.is_file() {
                let receipt: InstallReceipt = serde_json::from_slice(&fs::read(receipt_path)?)?;
                if receipt.target == target
                    && receipt.phase == InstallationPhase::Verified
                    && receipt.package_sha256 == active_hash
                    && receipt.version == request.version
                    && receipt.binary_sha256 == request.binary_sha256
                    && serde_json::to_value(&receipt.evidence)?
                        == serde_json::to_value(&request.evidence)?
                {
                    let provenance: Value = serde_json::from_slice(&fs::read(
                        target.join("agent-research-install.json"),
                    )?)?;
                    if provenance["source_package_sha256"].as_str() == Some(&source_hash) {
                        return Ok(receipt);
                    }
                }
            }
        }
    }
    let operation_id = unique_id();
    let operation = backup_root.join(format!("{}-{operation_id}", request.skill_directory_name));
    fs::create_dir(&operation)?;
    let staging = operation.join("candidate");
    copy_package(&source, &staging)?;
    ensure!(
        tree_sha256(&staging)? == source_hash,
        "package changed while staging"
    );
    // Metadata is part of the installed package hash; local development provenance is explicit.
    fs::write(
        staging.join("agent-research-install.json"),
        serde_json::to_vec_pretty(&json!({
            "version": request.version, "development_build": true, "source_package_sha256": source_hash,
            "binary_sha256": request.binary_sha256, "evidence": request.evidence,
        }))?,
    )?;
    let mut receipt = InstallReceipt {
        operation_id,
        target: target.clone(),
        backup: target.exists().then(|| operation.join("previous")),
        previous_sha256: if target.exists() {
            Some(tree_sha256(&target)?)
        } else {
            None
        },
        package_sha256: tree_sha256(&staging)?,
        binary_sha256: request.binary_sha256.clone(),
        binary_relative_path: request.binary_relative_path.clone(),
        version: request.version.clone(),
        evidence: request.evidence.clone(),
        phase: InstallationPhase::Staged,
        receipt_path: operation.join("receipt.json"),
    };
    write_receipt(&receipt)?;
    // Preparation can take time. Do not activate a candidate if its tested
    // default-branch base advanced while its package was being staged.
    skills_manifest.verify_skills(remote)?;
    remote.verify_remote_binding(upstream)?;
    if let Some(backup) = &receipt.backup {
        fs::rename(&target, backup)
            .context("backup and skills directory must be on the same filesystem")?;
        receipt.phase = InstallationPhase::PreviousBackedUp;
        write_receipt(&receipt)?;
    }
    if let Err(error) = fs::rename(&staging, &target) {
        if let Some(backup) = &receipt.backup {
            fs::rename(backup, &target)?;
        }
        bail!("candidate activation failed; previous installation restored: {error}");
    }
    receipt.phase = InstallationPhase::Activated;
    write_receipt(&receipt)?;
    let smoke = capture_text(
        target
            .join(&request.binary_relative_path)
            .to_str()
            .context("non-UTF-8 binary path")?,
        &request.smoke_arguments,
        &target,
    );
    if let Err(error) = smoke {
        rollback_after_smoke(&mut receipt)?;
        bail!("candidate smoke test failed and was rolled back: {error}");
    }
    if file_sha256(&target.join(&request.binary_relative_path))? != request.binary_sha256
        || tree_sha256(&target)? != receipt.package_sha256
    {
        rollback_after_smoke(&mut receipt)?;
        bail!("candidate changed its installed files during smoke testing and was rolled back");
    }
    if let Err(error) = remote.verify_remote_binding(upstream) {
        rollback_after_smoke(&mut receipt)?;
        bail!(
            "upstream changed or could not be verified during installation; candidate rolled back: {error}"
        );
    }
    receipt.phase = InstallationPhase::Verified;
    write_receipt(&receipt)?;
    Ok(receipt)
}

fn rollback_after_smoke(receipt: &mut InstallReceipt) -> Result<()> {
    // The install lock is still held and this is the just-started candidate; preserve any
    // files it wrote as rejected evidence while recovering the verified previous directory.
    rollback_checked(receipt, false)
}

fn rollback_locked(receipt: &mut InstallReceipt) -> Result<()> {
    rollback_checked(receipt, true)
}

fn rollback_checked(receipt: &mut InstallReceipt, verify_active: bool) -> Result<()> {
    if receipt.phase == InstallationPhase::RolledBack {
        return Ok(());
    }
    if receipt.target.exists() && verify_active {
        let active = tree_sha256(&receipt.target)?;
        if Some(&active) == receipt.previous_sha256.as_ref() {
            // Either activation never started, or restore completed before the journal write.
            receipt.phase = InstallationPhase::RolledBack;
            return write_receipt(receipt);
        }
        ensure!(
            active == receipt.package_sha256,
            "installed files changed; refusing to overwrite another version"
        );
    }
    if let Some(backup) = &receipt.backup {
        ensure!(
            Some(tree_sha256(backup)?) == receipt.previous_sha256,
            "previous installation backup changed"
        );
    }
    let failed = receipt
        .receipt_path
        .parent()
        .context("missing receipt directory")?
        .join("rejected");
    if receipt.target.exists() {
        fs::rename(&receipt.target, &failed)?;
    }
    if let Some(backup) = &receipt.backup {
        fs::rename(backup, &receipt.target)?;
    }
    receipt.phase = InstallationPhase::RolledBack;
    write_receipt(receipt)
}

/// Read the durable receipt from disk; refuse rollback if a later installation changed the target.
pub fn rollback_install(receipt_path: &Path) -> Result<InstallReceipt> {
    let mut receipt: InstallReceipt = serde_json::from_slice(&fs::read(receipt_path)?)?;
    ensure!(
        fs::canonicalize(receipt_path)? == fs::canonicalize(&receipt.receipt_path)?,
        "receipt path mismatch"
    );
    let root = receipt_path
        .parent()
        .and_then(Path::parent)
        .context("invalid receipt location")?;
    let name = receipt
        .target
        .file_name()
        .and_then(|v| v.to_str())
        .context("invalid installation name")?;
    let _lock = installation_lock(root, name)?;
    rollback_locked(&mut receipt)?;
    Ok(receipt)
}

#[cfg(test)]
#[path = "installation_tests.rs"]
mod tests;
