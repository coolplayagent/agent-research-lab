//! Replaceable protocol adapters. Verification commands are explicitly selected by the host.
mod git_knowledge;
use anyhow::{Context, Result, ensure};
use contracts::{OutcomeKind, OutcomeServiceKind};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use service_api::{OutcomeServiceHandshake, OutcomeServiceRequest, SandboxRequest};
use service_protocol as rpc;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Configuration {
    Verifier {
        id: String,
        outcome_kinds: Vec<OutcomeKind>,
        sandbox: PathBuf,
        command: Vec<String>,
        env_names: Vec<String>,
        read_only_paths: Vec<PathBuf>,
        network: bool,
    },
    Knowledge {
        id: String,
        repositories: BTreeMap<String, GitTarget>,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GitTarget {
    pub path: PathBuf,
    pub reference: String,
}
impl Configuration {
    fn info(&self) -> OutcomeServiceHandshake {
        let (id, kind, kinds) = match self {
            Self::Verifier {
                id, outcome_kinds, ..
            } => (id, OutcomeServiceKind::Verifier, outcome_kinds.clone()),
            Self::Knowledge { id, .. } => {
                (id, OutcomeServiceKind::Knowledge, OutcomeKind::ALL.to_vec())
            }
        };
        OutcomeServiceHandshake {
            protocol_version: crystal::OUTCOME_PROTOCOL_VERSION,
            id: id.clone(),
            kind,
            outcome_kinds: kinds,
            ready: true,
        }
    }
    fn validate(&self) -> Result<()> {
        crystal::id(&self.info().id)?;
        match self {
            Self::Verifier {
                outcome_kinds,
                sandbox,
                command,
                env_names,
                read_only_paths,
                ..
            } => {
                rpc::endpoint(sandbox)?;
                ensure!(
                    !outcome_kinds.is_empty() && outcome_kinds.len() <= 4,
                    "invalid verifier capabilities"
                );
                ensure!(
                    !command.is_empty()
                        && command.len() <= 64
                        && command.iter().all(|v| !v.is_empty() && v.len() <= 4096),
                    "invalid verifier command"
                );
                ensure!(
                    env_names.len() <= 32 && read_only_paths.len() <= 32,
                    "verifier configuration exceeds bound"
                );
            }
            Self::Knowledge { repositories, .. } => {
                ensure!(
                    !repositories.is_empty() && repositories.len() <= 32,
                    "invalid repository count"
                );
                for (id, target) in repositories {
                    crystal::id(id)?;
                    ensure!(
                        target.path.is_absolute() && target.path.is_dir(),
                        "knowledge repository must exist outside the service store"
                    );
                    ensure!(
                        target.reference.starts_with("refs/heads/")
                            && target.reference.len() <= 256,
                        "prepared knowledge branch required"
                    );
                }
            }
        }
        Ok(())
    }
}
async fn dispatch(
    config: &Configuration,
    state: &Path,
    request: OutcomeServiceRequest,
) -> Result<Value> {
    match request {
        OutcomeServiceRequest::Probe => {
            if let Configuration::Verifier { sandbox, .. } = config {
                service_api::probe(sandbox, contracts::ServiceKind::Sandbox).await?;
            }
            Ok(json!(config.info()))
        }
        OutcomeServiceRequest::Verify { run, outcome } => {
            outcome.validate_digest()?;
            let Configuration::Verifier {
                id,
                outcome_kinds,
                sandbox,
                command,
                env_names,
                read_only_paths,
                network,
            } = config
            else {
                anyhow::bail!("service is not a verifier");
            };
            ensure!(
                run.input.verifier_id == *id && outcome_kinds.contains(&outcome.input.kind),
                "verifier binding mismatch"
            );
            ensure!(
                run.input.outcome_digest == outcome.digest
                    && run.input.outcome_id == outcome.input.id
                    && run.request_digest == storage::digest(&serde_json::to_vec(&run.input)?),
                "verification request digest mismatch"
            );
            ensure!(
                (1..=600).contains(&run.input.max_seconds),
                "invalid verification limit"
            );
            let seconds = ((run.deadline_ms.saturating_sub(crystal::now_ms())) / 1000)
                .min(run.input.max_seconds);
            ensure!(seconds > 0, "verification deadline expired");
            let output: service_api::SandboxOutput = rpc::call(
                sandbox,
                &SandboxRequest::Run {
                    command: command.clone(),
                    env_names: env_names.clone(),
                    read_only_paths: read_only_paths.clone(),
                    network: *network,
                    input: serde_json::to_string(&OutcomeServiceRequest::Verify {
                        run: run.clone(),
                        outcome,
                    })?,
                    seconds,
                },
                Duration::from_secs(seconds + 2),
            )
            .await?;
            let result: crystal::OutcomeVerificationResult = serde_json::from_str(&output.stdout)?;
            ensure!(
                result.id == run.input.id && result.request_digest == run.request_digest,
                "verification response identity mismatch"
            );
            Ok(json!(result))
        }
        request @ (OutcomeServiceRequest::Write { .. }
        | OutcomeServiceRequest::LookupWrite { .. }) => {
            let Configuration::Knowledge { id, repositories } = config else {
                anyhow::bail!("service is not a knowledge publisher");
            };
            let (id, repositories, state) = (id.clone(), repositories.clone(), state.to_path_buf());
            tokio::task::spawn_blocking(move || {
                git_knowledge::dispatch(&id, &repositories, &state, request)
            })
            .await?
        }
    }
}
pub async fn serve(config: Configuration, state: &Path, socket: &Path, seconds: u64) -> Result<()> {
    config.validate()?;
    ensure!((1..=43200).contains(&seconds), "invalid service lifetime");
    std::fs::create_dir_all(state)?;
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(state, std::fs::Permissions::from_mode(0o700))?;
    let _owner = storage::lock(&state.join("owner.lock"))?;
    let listener = rpc::Listener::bind(socket)?;
    let permits = Arc::new(tokio::sync::Semaphore::new(4));
    let mut tasks = tokio::task::JoinSet::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(seconds);
    loop {
        tokio::select! {
            _ = tokio::time::sleep_until(deadline) => break,
            Some(_) = tasks.join_next(), if !tasks.is_empty() => {},
            stream = listener.accept() => {
                let mut stream = stream?;
                let Ok(permit) = permits.clone().try_acquire_owned() else {
                    let _ = tokio::time::timeout(Duration::from_secs(1), rpc::write(&mut stream, &rpc::Reply::from_result(Err(anyhow::anyhow!("service capacity backpressure"))))).await;
                    continue;
                };
                let (config, state) = (config.clone(), state.to_path_buf());
                tasks.spawn(async move {
                    let _permit = permit;
                    let result = async {
                        let envelope: rpc::Envelope<OutcomeServiceRequest> = tokio::time::timeout(Duration::from_secs(5), rpc::read(&mut stream)).await??;
                        ensure!(envelope.protocol_version == rpc::VERSION, "unsupported protocol");
                        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
                        ensure!(remaining.as_secs() >= 605, "service lifetime is too short for an operation");
                        tokio::time::timeout(Duration::from_secs(602), dispatch(&config, &state, envelope.request)).await?
                    }.await;
                    let _ = tokio::time::timeout(Duration::from_secs(5), rpc::write(&mut stream, &rpc::Reply::from_result(result))).await;
                });
            }
        }
    }
    while tasks.join_next().await.is_some() {}
    Ok(())
}
