//! Ordered platform rollout and rollback with offline storage compatibility checks.
use crate::{
    admission::{begin_activation, drain},
    host::{command, copy_tree, immutable, regular, relative, write_bytes},
    journal::{bound_receipt, mark_switch, phase},
    model::{Prepared, SwitchProof, UnitProof},
    service::{executable, replace_executable, running_artifact, service_health, systemctl, unit},
};
use anyhow::{Context, Result, ensure};
use config::Config;
use contracts::ActivationPhase;
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

fn platform_start(prepared: &Prepared) -> Result<()> {
    for (index, unit) in prepared.repository.platform_units.iter().enumerate() {
        systemctl(&["start".into(), unit.unit.clone()])?;
        if index == 0 {
            let socket = prepared
                .policy
                .storage_socket
                .as_ref()
                .context("platform storage socket required")?;
            let client = collaboration_client::Client::remote(socket.clone())?;
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;
            let start = Instant::now();
            loop {
                if runtime
                    .block_on(client.probe())
                    .is_ok_and(|info| info.ready && info.kind == contracts::ServiceKind::Storage)
                {
                    break;
                }
                ensure!(
                    start.elapsed() < Duration::from_secs(30),
                    "platform storage did not become ready"
                );
                std::thread::sleep(Duration::from_millis(250));
            }
        }
    }
    service_health(&prepared.policy)
}

pub(super) fn activate_platform(
    operation: &Path,
    prepared: &Prepared,
    config: &Config,
    source: &Path,
) -> Result<Value> {
    let units = &prepared.repository.platform_units;
    ensure!(
        units.len() == 4
            && units[0].unit == "outcome-hub-storage"
            && units[1].unit == "outcome-hub-executor"
            && units[2].unit == "outcome-hub-sandbox"
            && units[3].unit == prepared.policy.main_unit,
        "platform units must be ordered storage, executor, sandbox, main"
    );
    let storage_state = prepared
        .policy
        .storage_state_dir
        .as_ref()
        .context("platform storage data path required")?;
    ensure!(
        storage_state.is_absolute(),
        "platform storage data path must be absolute"
    );
    let release = operation.join("release");
    fs::create_dir(&release)?;
    let mut previous = Vec::new();
    let mut artifacts = Vec::new();
    for specification in units {
        unit(&specification.unit)?;
        relative(&specification.artifact)?;
        ensure!(
            specification.unit_file.is_absolute(),
            "platform unit file must be absolute"
        );
        let content = String::from_utf8(regular(&specification.unit_file, 65536)?)?;
        executable(&content)?;
        let binary = release.join(&specification.unit);
        let built = source.join(&specification.artifact);
        ensure!(
            fs::metadata(&built)?.is_file(),
            "platform build artifact missing"
        );
        fs::copy(&built, &binary)?;
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o555))?;
        command(
            &[binary.display().to_string(), "--help".into()],
            &release,
            10,
        )?;
        previous.push(content);
        artifacts.push(binary);
    }
    immutable(&release)?;
    for (index, content) in previous.iter().enumerate() {
        write_bytes(
            &operation.join(format!("unit-{index}.previous")),
            content.as_bytes(),
        )?;
    }
    let _admission = drain(operation, prepared, config)?;
    for (index, specification) in units.iter().enumerate() {
        ensure!(
            regular(&specification.unit_file, 65536)? == previous[index].as_bytes(),
            "service unit changed during activation preparation"
        );
    }
    let mut proof_units = Vec::new();
    for (index, specification) in units.iter().enumerate() {
        let previous_artifact_sha256 =
            delivery::file_sha256(Path::new(executable(&previous[index])?))?;
        ensure!(
            running_artifact(&specification.unit)? == previous_artifact_sha256,
            "platform previous unit does not describe its running artifact"
        );
        proof_units.push(UnitProof {
            unit: specification.unit.clone(),
            unit_file: specification.unit_file.clone(),
            previous_unit_sha256: storage::digest(previous[index].as_bytes()),
            next_unit_sha256: storage::digest(
                replace_executable(&previous[index], &artifacts[index])?.as_bytes(),
            ),
            previous_artifact_sha256,
            next_artifact_sha256: delivery::file_sha256(&artifacts[index])?,
        });
    }
    storage::write(
        &operation.join("switch-proof.json"),
        &SwitchProof {
            previous_config_sha256: prepared.config_sha256.clone(),
            next_config_sha256: prepared.config_sha256.clone(),
            main_artifact_sha256: running_artifact(&prepared.policy.main_unit)?,
            artifact: artifacts[3].clone(),
            artifact_sha256: delivery::file_sha256(&artifacts[3])?,
            units: proof_units,
        },
    )?;
    begin_activation(operation, prepared)?;
    mark_switch(operation)?;
    phase(
        operation,
        ActivationPhase::Switching,
        json!({"service_count":units.len()}),
    )?;
    let rollout = (|| -> Result<()> {
        for specification in units.iter().rev() {
            systemctl(&["stop".into(), specification.unit.clone()])?;
        }
        // Both storage versions own a separate disposable copy during compatibility checks.
        // A migration that the previous service cannot read is blocked before live data changes.
        let snapshot = operation.join("storage-compatibility");
        copy_tree(storage_state, &snapshot, &mut 0, &mut 0)?;
        let socket = prepared
            .state_dir
            .join("private/activation-checks")
            .join(format!(
                "{}.sock",
                &storage::digest(prepared.request.goal_id.as_bytes())[..16]
            ));
        for binary in [&artifacts[0], &PathBuf::from(executable(&previous[0])?)] {
            command(
                &[
                    binary.display().to_string(),
                    "--state-dir".into(),
                    snapshot.display().to_string(),
                    "--socket".into(),
                    socket.display().to_string(),
                    "--max-seconds".into(),
                    "1".into(),
                ],
                operation,
                10,
            )?;
        }
        for (index, specification) in units.iter().enumerate() {
            write_bytes(
                &specification.unit_file,
                replace_executable(&previous[index], &artifacts[index])?.as_bytes(),
            )?;
        }
        systemctl(&["daemon-reload".into()])?;
        phase(operation, ActivationPhase::Checking, json!({}))?;
        platform_start(prepared)?;
        for (index, specification) in units.iter().enumerate() {
            let pid = String::from_utf8(systemctl(&[
                "show".into(),
                "--property=MainPID".into(),
                "--value".into(),
                specification.unit.clone(),
            ])?)?
            .trim()
            .parse::<u32>()?;
            ensure!(
                pid > 0
                    && delivery::file_sha256(Path::new(&format!("/proc/{pid}/exe")))?
                        == delivery::file_sha256(&artifacts[index])?,
                "platform service runs a different artifact"
            );
        }
        Ok(())
    })();
    if let Err(error) = rollout {
        for specification in units.iter().rev() {
            let _ = systemctl(&["stop".into(), specification.unit.clone()]);
        }
        for (index, specification) in units.iter().enumerate() {
            write_bytes(&specification.unit_file, previous[index].as_bytes())?;
        }
        systemctl(&["daemon-reload".into()])?;
        platform_start(prepared)?;
        phase(
            operation,
            ActivationPhase::RolledBack,
            json!({"error":error.to_string()}),
        )?;
        anyhow::bail!(
            "platform activation failed; previous service artifacts restored without replacing live data: {error}"
        );
    }
    bound_receipt(
        operation,
        prepared,
        true,
        &artifacts[3],
        &delivery::file_sha256(&artifacts[3])?,
    )
}
