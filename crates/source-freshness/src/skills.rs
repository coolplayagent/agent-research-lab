//! Verify complete skill manifests and local attempt-bound installed bytes.
use crate::{
    client::RemoteClient,
    files::{hex, regular_file_digest, skill_tree_digest},
    identity::{trusted_repository, validate_hex},
    protocol::SkillsManifest,
    source_gate::verify_source_qualitygate,
};
use anyhow::{Context, Result, bail, ensure};
use chrono::Utc;
use std::path::Path;

pub fn verify_skills(manifest_path: &Path) -> Result<SkillsManifest> {
    RemoteClient::default().verify_skills(manifest_path)
}

/// Local boundary check around a model attempt. A concurrent host installation
/// invalidates its observations instead of silently mixing tool versions.
pub fn verify_installed_bytes(manifest: &SkillsManifest) -> Result<()> {
    for skill in &manifest.skills {
        ensure!(
            skill_tree_digest(&skill.installed_path)? == skill.tree_sha256,
            "skill changed during experiment: {}",
            skill.name
        );
        let (digest, _) = regular_file_digest(&skill.runtime_path)?;
        ensure!(
            hex(&digest) == skill.runtime_sha256,
            "CLI changed during experiment: {}",
            skill.name
        );
    }
    Ok(())
}

impl RemoteClient {
    pub fn verify_skills(&self, manifest_path: &Path) -> Result<SkillsManifest> {
        let mut manifest: SkillsManifest = storage::read(manifest_path)?;
        ensure!(
            manifest.schema_version == 1 && !manifest.skills.is_empty(),
            "invalid or empty skills manifest"
        );
        let mut names = std::collections::BTreeSet::new();
        for skill in &manifest.skills {
            config::safe_id(&skill.name)?;
            ensure!(names.insert(skill.name.clone()), "duplicate skill name");
        }
        manifest.skills = process::parallel_map(&manifest.skills, |skill| {
            let mut skill = skill.clone();
            trusted_repository(&skill.repository)?;
            validate_hex(&skill.tree_sha256, 64)?;
            validate_hex(&skill.runtime_sha256, 64)?;
            chrono::DateTime::parse_from_rfc3339(&skill.checked_at)
                .context("invalid skill freshness timestamp")?;
            ensure!(
                skill.installed_path.is_absolute() && skill.runtime_path.is_absolute(),
                "skill/runtime paths must be absolute"
            );
            ensure!(
                skill.installed_path.join("SKILL.md").is_file(),
                "installed skill lacks SKILL.md"
            );
            ensure!(
                skill_tree_digest(&skill.installed_path)? == skill.tree_sha256,
                "installed skill tree differs from host manifest: {}",
                skill.name
            );
            let (runtime_digest, _) = regular_file_digest(&skill.runtime_path)?;
            ensure!(
                hex(&runtime_digest) == skill.runtime_sha256,
                "installed CLI differs from host manifest: {}",
                skill.name
            );
            match (&skill.release_tag, skill.release_id, &skill.source_commit) {
                (Some(tag), Some(id), None) if !tag.is_empty() && id > 0 => {
                    ensure!(
                        skill.source_qualitygate.is_none(),
                        "release skill cannot carry a source override"
                    );
                    ensure!(
                        self.latest_release(&skill.repository)? == Some((id, tag.clone())),
                        "installed skill is not the latest published release: {}",
                        skill.name
                    );
                }
                (None, None, Some(commit)) => {
                    validate_hex(commit, 40)?;
                    let release_exists = self.latest_release(&skill.repository)?.is_some();
                    if release_exists || skill.source_qualitygate.is_some() {
                        verify_source_qualitygate(&skill)?;
                    }
                    let latest = self.remote_binding(&skill.repository)?;
                    ensure!(
                        latest.commit == *commit,
                        "source-installed skill is stale: {}",
                        skill.name
                    );
                }
                _ => bail!("skill must bind exactly one release (tag+ID) or source commit"),
            }
            skill.checked_at = Utc::now().to_rfc3339();
            Ok(skill)
        })?;
        Ok(manifest)
    }
}

#[cfg(test)]
#[path = "skills_tests.rs"]
mod tests;
