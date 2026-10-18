//! Exact-commit full gate proof for a host-built source skill.
use crate::{
    files::{hex, reject_symlink_components},
    identity::validate_hex,
    protocol::SkillSnapshot,
};
use anyhow::{Context, Result, ensure};
use contracts::{
    QualitygateDecision, QualitygateProfile, QualitygateScope, QualitygateSnapshotMode,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fs, io::Read};

pub(super) fn verify_source_qualitygate(skill: &SkillSnapshot) -> Result<()> {
    let proof = skill.source_qualitygate.as_ref()
        .context("a published release exists; a source build requires an exact-commit full Qualitygate report")?;
    ensure!(
        proof.path.is_absolute(),
        "source Qualitygate path must be absolute"
    );
    validate_hex(&proof.sha256, 64)?;
    reject_symlink_components(&proof.path)?;
    use std::os::unix::fs::OpenOptionsExt;
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(&proof.path)?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file() && metadata.len() <= 16 * 1024 * 1024,
        "source Qualitygate report must be regular and at most 16 MiB"
    );
    let mut bytes = Vec::new();
    file.take(16 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 == metadata.len(),
        "source Qualitygate report changed size while reading"
    );
    // Hash and parse the exact same bounded read, even if a concurrent host
    // publication subsequently replaces the report pathname.
    ensure!(
        hex(&Sha256::digest(&bytes)) == proof.sha256,
        "source Qualitygate report changed"
    );
    let report: Value = serde_json::from_slice(&bytes)?;
    ensure!(
        QualitygateProfile::from_value(&report["profile"]) == Some(QualitygateProfile::Full)
            && matches!(
                QualitygateScope::from_value(&report["scope"]),
                Some(QualitygateScope::Delivery | QualitygateScope::Task)
            )
            && report["gate"]["complete"] == true
            && QualitygateDecision::from_value(&report["gate"]["decision"])
                == Some(QualitygateDecision::Pass)
            && report["plan"]["pending_delivery_checks"]
                .as_array()
                .is_some_and(Vec::is_empty)
            && report["selection"]["empty_delivery"] == false
            && QualitygateSnapshotMode::from_value(&report["snapshot"]["mode"])
                == Some(QualitygateSnapshotMode::Diff)
            && report["snapshot"]["head"].as_str() == skill.source_commit.as_deref(),
        "source skill needs a complete, nonempty full diff gate at its exact source commit"
    );
    Ok(())
}

#[cfg(test)]
#[path = "source_gate_tests.rs"]
mod tests;
