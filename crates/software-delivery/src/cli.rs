//! Prepare an immutable skill package and switch its service configuration.
use crate::{
    admission::{begin_activation, drain},
    host::{command, copy_tree, immutable, regular, relative, write_bytes},
    journal::{bound_receipt, mark_switch, phase},
    model::{Prepared, SwitchProof},
    service::{running_artifact, service_health, systemctl},
};
use anyhow::{Context, Result, ensure};
use config::Config;
use contracts::ActivationPhase;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn prepare_cli(
    operation: &Path,
    prepared: &Prepared,
    config: &Config,
    source: &Path,
) -> Result<(Config, PathBuf)> {
    relative(&prepared.repository.package_directory)?;
    relative(&prepared.repository.binary_relative_path)?;
    ensure!(
        !prepared.repository.probe.is_empty(),
        "activation binary probe is required"
    );
    let package = operation.join("release");
    copy_tree(
        &source.join(&prepared.repository.package_directory),
        &package,
        &mut 0,
        &mut 0,
    )?;
    ensure!(
        package.join("SKILL.md").is_file(),
        "built package lacks SKILL.md"
    );
    let binary = package.join(&prepared.repository.binary_relative_path);
    ensure!(
        fs::metadata(&binary)?.is_file(),
        "built skill package lacks its configured binary"
    );
    let probe = command(
        &[
            vec![binary.display().to_string()],
            prepared.repository.probe.clone(),
        ]
        .concat(),
        &package,
        30,
    )?;
    fs::write(operation.join("probe.log"), probe)?;
    immutable(&package)?;
    let mut manifest: freshness::SkillsManifest = storage::read(
        config
            .skills_manifest
            .as_ref()
            .context("skills manifest required")?,
    )?;
    let mut next = config.clone();
    let skill = manifest
        .skills
        .iter_mut()
        .find(|skill| skill.repository == prepared.request.repository)
        .context("repository has no installed skill binding")?;
    let tool = next
        .tools
        .get_mut(&skill.name)
        .context("skill has no configured CLI")?;
    tool.binary = binary.clone();
    if skill.name == "workflow-cli" {
        next.workflow = binary.clone();
    }
    skill.installed_path = package;
    skill.runtime_path = binary.clone();
    skill.tree_sha256 = freshness::skill_tree_digest(&skill.installed_path)?;
    skill.runtime_sha256 = delivery::file_sha256(&binary)?;
    skill.release_tag = None;
    skill.release_id = None;
    skill.source_commit = Some(prepared.request.merged_commit.clone());
    skill.source_qualitygate = Some(freshness::SourceQualitygate {
        path: operation.join("merged-qualitygate.json"),
        sha256: delivery::file_sha256(&operation.join("merged-qualitygate.json"))?,
    });
    skill.checked_at = chrono::Utc::now().to_rfc3339();
    let manifest_path = operation.join("skills.json");
    storage::write(&manifest_path, &manifest)?;
    freshness::RemoteClient::default().verify_skills(&manifest_path)?;
    next.skills_manifest = Some(manifest_path);
    next.validate()?;
    Ok((next, binary))
}

pub(super) fn activate_cli(
    operation: &Path,
    prepared: &Prepared,
    config: &Config,
    source: &Path,
) -> Result<Value> {
    let (next, binary) = prepare_cli(operation, prepared, config, source)?;
    let _admission = drain(operation, prepared, config)?;
    ensure!(
        delivery::file_sha256(&prepared.policy.service_config)? == prepared.config_sha256,
        "service config changed during preparation"
    );
    let previous = regular(&prepared.policy.service_config, 1024 * 1024)?;
    write_bytes(&operation.join("previous.toml"), &previous)?;
    let next_bytes = next.to_toml()?.into_bytes();
    storage::write(
        &operation.join("switch-proof.json"),
        &SwitchProof {
            previous_config_sha256: storage::digest(&previous),
            next_config_sha256: storage::digest(&next_bytes),
            main_artifact_sha256: running_artifact(&prepared.policy.main_unit)?,
            artifact: binary.clone(),
            artifact_sha256: delivery::file_sha256(&binary)?,
            units: vec![],
        },
    )?;
    begin_activation(operation, prepared)?;
    mark_switch(operation)?;
    phase(
        operation,
        ActivationPhase::Switching,
        json!({"binary_sha256":delivery::file_sha256(&binary)?}),
    )?;
    write_bytes(&prepared.policy.service_config, &next_bytes)?;
    let restarted =
        systemctl(&["restart".into(), prepared.policy.main_unit.clone()]).and_then(|_| {
            phase(operation, ActivationPhase::Checking, json!({}))?;
            service_health(&prepared.policy)
        });
    if let Err(error) = restarted {
        write_bytes(&prepared.policy.service_config, &previous)?;
        systemctl(&["restart".into(), prepared.policy.main_unit.clone()])?;
        service_health(&prepared.policy)?;
        phase(
            operation,
            ActivationPhase::RolledBack,
            json!({"error":error.to_string()}),
        )?;
        anyhow::bail!("activation failed; previous config and tools restored: {error}");
    }
    bound_receipt(
        operation,
        prepared,
        true,
        &binary,
        &delivery::file_sha256(&binary)?,
    )
}
