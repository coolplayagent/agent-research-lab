//! Admit a frozen request or inspect its existing bounded worker operation.
use crate::{
    admission::{confirmed_claim, release_drain},
    host::{command, regular},
    journal::{activation_claim, phase},
    model::{Policy, Prepared, Request},
    service::{systemctl, unit},
};
use anyhow::{Context, Result, ensure};
use config::Config;
use contracts::ActivationPhase;
use serde_json::{Value, json};
use std::{fs, path::Path};

pub fn request(config_path: &Path, input: &Path, policy_digest: &str) -> Result<Value> {
    delivery::validate_digest(policy_digest)?;
    let config_path = fs::canonicalize(config_path)?;
    let config = Config::load(&config_path)?;
    let request: Request = serde_json::from_slice(&regular(input, 1024 * 1024)?)?;
    ensure!(
        request.schema_version == 1,
        "unsupported activation request"
    );
    for id in [&request.iteration_id, &request.goal_id] {
        config::safe_id(id)?;
    }
    delivery::validate_commit(&request.candidate_commit)?;
    delivery::validate_commit(&request.merged_commit)?;
    request.evidence.validate()?;
    ensure!(
        request.evidence.source_commit == request.candidate_commit,
        "activation evidence belongs to another candidate"
    );
    let bytes = regular(
        &config.state_dir.join("private/software-activation.json"),
        65536,
    )?;
    ensure!(
        storage::digest(&bytes) == policy_digest,
        "activation policy differs from its frozen digest"
    );
    let policy: Policy = serde_json::from_slice(&bytes)?;
    ensure!(
        policy.schema_version == 1 && fs::canonicalize(&policy.service_config)? == config_path,
        "activation policy config mismatch"
    );
    unit(&policy.main_unit)?;
    ensure!(
        policy
            .storage_socket
            .as_ref()
            .is_some_and(|path| path.is_absolute()),
        "activation requires an absolute storage socket"
    );
    ensure!(
        policy.health_url.starts_with("http://") && !policy.health_url.contains(['\n', '\r']),
        "activation health URL must use local HTTP"
    );
    let repository = policy
        .repositories
        .get(&request.repository)
        .context("repository activation is not configured")?
        .clone();
    ensure!(
        repository.checkout.is_absolute()
            && !repository.build_commands.is_empty()
            && repository.build_commands.len() <= 16,
        "activation requires a bounded configured build"
    );
    let key = storage::digest(&serde_json::to_vec(&request)?);
    let operation = config
        .state_dir
        .join("private/software-activation")
        .join(&key[..40]);
    let _lock = storage::lock(
        &config
            .state_dir
            .join("private/software-activation-request.lock"),
    )?;
    if operation.join("receipt.json").exists() {
        let receipt: Value = storage::read(&operation.join("receipt.json"))?;
        ensure!(
            receipt["goal_id"] == request.goal_id
                && receipt["candidate_commit"] == request.candidate_commit
                && receipt["merged_commit"] == request.merged_commit,
            "activation receipt identity mismatch"
        );
        return Ok(receipt);
    }
    if operation.join("prepared.json").exists() {
        let mut status: Value = storage::read(&operation.join("status.json"))?;
        let terminal = matches!(
            ActivationPhase::from_value(&status["phase"]),
            Some(ActivationPhase::Blocked | ActivationPhase::RolledBack)
        );
        if !terminal
            && status["observed_ms"]
                .as_u64()
                .is_some_and(|time| collaboration_model::now_ms().saturating_sub(time) > 60000)
            && systemctl(&[
                "is-active".into(),
                format!("ai-im-activation-{}", &key[..24]),
            ])
            .is_err()
        {
            phase(
                &operation,
                ActivationPhase::Blocked,
                json!({"error":"Activation worker stopped before its receipt was committed; reconcile actual service state before retrying."}),
            )?;
            status = storage::read(&operation.join("status.json"))?;
        }
        let mut receipt = json!({"schema_version":1,"goal_id":request.goal_id,"candidate_commit":request.candidate_commit,"merged_commit":request.merged_commit,"active":false,"phase":status["phase"],"details":status["details"],"operation_id":key});
        if let Some(claim) = activation_claim(&operation)? {
            let prepared: Prepared = storage::read(&operation.join("prepared.json"))?;
            let blocked =
                ActivationPhase::from_value(&status["phase"]) == Some(ActivationPhase::Blocked);
            if blocked && confirmed_claim(&operation, &prepared).is_ok_and(|claim| claim.is_none())
            {
                release_drain(&operation, &prepared)?;
            } else {
                receipt["activation_claim_id"] = json!(claim);
                receipt["requires_reconciliation"] = json!(blocked);
            }
        }
        return Ok(receipt);
    }
    fs::create_dir_all(&operation)?;
    let prepared = Prepared {
        request,
        repository,
        policy,
        state_dir: config.state_dir.clone(),
        config_sha256: delivery::file_sha256(&config_path)?,
        policy_sha256: policy_digest.into(),
    };
    storage::write(&operation.join("prepared.json"), &prepared)?;
    phase(&operation, ActivationPhase::Prepared, json!({}))?;
    let executable = std::env::current_exe()?;
    let mut args = vec![
        "systemd-run".into(),
        "--user".into(),
        "--collect".into(),
        format!("--unit=ai-im-activation-{}", &key[..24]),
        "--property=RuntimeMaxSec=7200".into(),
        "--property=TimeoutStopSec=30".into(),
        "--property=MemoryMax=8G".into(),
        "--property=TasksMax=512".into(),
        "--property=UMask=0077".into(),
        format!("--working-directory={}", config.workspace.display()),
        format!("--setenv=PATH={}", std::env::var("PATH")?),
        executable.display().to_string(),
        "software-activation-worker".into(),
        operation.display().to_string(),
    ];
    if config.state_dir.join("service.env").is_file() {
        args.insert(
            10,
            format!(
                "--property=EnvironmentFile={}",
                config.state_dir.join("service.env").display()
            ),
        );
    }
    if let Err(error) = command(&args, &config.workspace, 30) {
        phase(
            &operation,
            ActivationPhase::Blocked,
            json!({"error":error.to_string()}),
        )?;
        return Err(error);
    }
    Ok(
        json!({"schema_version":1,"goal_id":prepared.request.goal_id,"candidate_commit":prepared.request.candidate_commit,"merged_commit":prepared.request.merged_commit,"active":false,"phase":ActivationPhase::Prepared,"operation_id":key}),
    )
}
