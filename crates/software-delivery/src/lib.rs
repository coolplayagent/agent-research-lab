//! Host-owned, bounded activation of an exact merged software snapshot.
use anyhow::{Context, Result, ensure};
use config::Config;
use contracts::{ActivationPhase, ActivationState, CandidateState, IterationState, RoundState};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Component, Path, PathBuf},
    time::{Duration, Instant},
};

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Policy {
    schema_version: u32,
    service_config: PathBuf,
    main_unit: String,
    health_url: String,
    repositories: BTreeMap<String, Repository>,
    #[serde(default)]
    storage_state_dir: Option<PathBuf>,
    #[serde(default)]
    storage_socket: Option<PathBuf>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Repository {
    checkout: PathBuf,
    build_commands: Vec<Vec<String>>,
    #[serde(default)]
    package_directory: PathBuf,
    #[serde(default)]
    binary_relative_path: PathBuf,
    #[serde(default)]
    probe: Vec<String>,
    #[serde(default)]
    platform_units: Vec<PlatformUnit>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "software_verification::optional_budget"
    )]
    qualitygate_snapshot_max_file_mib: Option<software_verification::SnapshotMaxFileMib>,
}
impl Repository {
    fn qualitygate_command(&self, program: String, source: &Path, merged: &str) -> Vec<String> {
        software_verification::qualitygate_command(
            program,
            source,
            format!("{merged}^..{merged}"),
            self.qualitygate_snapshot_max_file_mib,
        )
    }
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PlatformUnit {
    unit: String,
    artifact: PathBuf,
    unit_file: PathBuf,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Request {
    schema_version: u32,
    iteration_id: String,
    goal_id: String,
    candidate_commit: String,
    merged_commit: String,
    repository: String,
    merge: Value,
    evidence: delivery::ReleaseEvidence,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Prepared {
    request: Request,
    policy: Policy,
    repository: Repository,
    state_dir: PathBuf,
    config_sha256: String,
    policy_sha256: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SwitchProof {
    previous_config_sha256: String,
    next_config_sha256: String,
    main_artifact_sha256: String,
    artifact: PathBuf,
    artifact_sha256: String,
    units: Vec<UnitProof>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct UnitProof {
    unit: String,
    unit_file: PathBuf,
    previous_unit_sha256: String,
    next_unit_sha256: String,
    previous_artifact_sha256: String,
    next_artifact_sha256: String,
}

fn storage_client(prepared: &Prepared) -> Result<im_client::Client> {
    im_client::Client::remote(
        prepared
            .policy
            .storage_socket
            .clone()
            .context("activation requires the collaboration storage endpoint")?,
    )
}
fn activation_claim(operation: &Path) -> Result<Option<String>> {
    let path = operation.join("admission.json");
    if !path.exists() {
        return Ok(None);
    }
    let change: collaboration_model::EvolutionChange = storage::read(&path)?;
    match change.action {
        collaboration_model::EvolutionAction::BeginActivation { claim_id, .. } => {
            Ok(Some(claim_id))
        }
        _ => anyhow::bail!("invalid activation admission journal"),
    }
}
fn confirmed_claim(operation: &Path, prepared: &Prepared) -> Result<Option<String>> {
    let Some(claim) = activation_claim(operation)? else {
        return Ok(None);
    };
    let client = storage_client(prepared)?;
    let run = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(client.evolution_run(&prepared.request.iteration_id))?;
    let round = run
        .rounds
        .iter()
        .find(|round| round.goal_id == prepared.request.goal_id)
        .context("activation goal is missing from its durable iteration")?;
    confirmed_round_claim(
        round,
        &claim,
        operation.join("switch-started.json").exists(),
    )
}
fn confirmed_round_claim(
    round: &collaboration_model::EvolutionRound,
    claim: &str,
    switched: bool,
) -> Result<Option<String>> {
    if round.activation.as_ref().is_some_and(|activation| {
        activation.state == ActivationState::Pending && activation.reference == claim
    }) {
        return Ok(Some(claim.into()));
    }
    ensure!(
        round.activation.is_none() && !switched,
        "activation admission does not match its durable round; reconcile actual service state"
    );
    Ok(None)
}
fn mark_switch(operation: &Path) -> Result<()> {
    storage::write(
        &operation.join("switch-started.json"),
        &json!({"activation_claim_id":activation_claim(operation)?.context("activation admission missing")?,
            "started_ms":collaboration_model::now_ms()}),
    )
}

fn begin_activation(operation: &Path, prepared: &Prepared) -> Result<String> {
    let client = storage_client(prepared)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let run = client.evolution_run(&prepared.request.iteration_id).await?;
        let round = run
            .rounds
            .last()
            .context("activation iteration has no round")?;
        let expected_version = format!(
            "candidate-{}",
            &storage::digest(
                format!(
                    "{}:{}",
                    run.input.subject_id, prepared.request.candidate_commit
                )
                .as_bytes()
            )[..40]
        );
        ensure!(
            run.state == IterationState::Running
                && round.state == RoundState::Accepted
                && round.goal_id == prepared.request.goal_id
                && round.finished_ms.is_none()
                && round
                    .candidate
                    .as_ref()
                    .is_some_and(|candidate| candidate.state == CandidateState::Accepted
                        && candidate.version_id == expected_version),
            "activation no longer owns the running accepted candidate"
        );
        let claim_id = format!(
            "activation-{}",
            &storage::digest(serde_json::to_vec(&prepared.request)?.as_slice())[..40]
        );
        let change = collaboration_model::EvolutionChange {
            request_id: claim_id.clone(),
            run_id: run.input.id,
            revision: run.revision,
            action: collaboration_model::EvolutionAction::BeginActivation {
                claim_id: claim_id.clone(),
                max_seconds: 3600,
            },
        };
        // Persist the exact command before RPC; an interrupted response cannot erase admission.
        storage::write(&operation.join("admission.json"), &change)?;
        client.evolution_change(change).await?;
        Ok(claim_id)
    })
}
fn main_pid(unit: &str) -> Result<u32> {
    let pid = String::from_utf8(systemctl(&[
        "show".into(),
        "--property=MainPID".into(),
        "--value".into(),
        unit.into(),
    ])?)?
    .trim()
    .parse::<u32>()?;
    ensure!(pid > 0, "service has no running main process");
    Ok(pid)
}
fn running_artifact(unit: &str) -> Result<String> {
    let pid = main_pid(unit)?;
    delivery::file_sha256(Path::new(&format!("/proc/{pid}/exe")))
}
fn config_loaded_after_write(policy: &Policy) -> Result<()> {
    let started = String::from_utf8(systemctl(&[
        "show".into(),
        "--property=ExecMainStartTimestampMonotonic".into(),
        "--value".into(),
        policy.main_unit.clone(),
    ])?)?
    .trim()
    .parse::<u64>()?;
    let uptime = fs::read_to_string("/proc/uptime")?
        .split_whitespace()
        .next()
        .context("kernel uptime is unavailable")?
        .parse::<f64>()?;
    let age = fs::metadata(&policy.service_config)?
        .modified()?
        .elapsed()?
        .as_micros();
    let written = ((uptime * 1_000_000.0) as u128).saturating_sub(age);
    ensure!(
        u128::from(started) >= written.saturating_add(20_000),
        "running service cannot be proven to have loaded the current config"
    );
    Ok(())
}
fn bound_receipt(
    operation: &Path,
    prepared: &Prepared,
    active: bool,
    reference: &Path,
    artifact_sha256: &str,
) -> Result<Value> {
    let claim = activation_claim(operation)?.context("activation admission receipt missing")?;
    Ok(
        json!({"schema_version":1,"goal_id":prepared.request.goal_id,
        "candidate_commit":prepared.request.candidate_commit,"merged_commit":prepared.request.merged_commit,
        "active":active,"phase":if active {ActivationPhase::Active} else {ActivationPhase::RolledBack},
        "activation_claim_id":claim,"artifact_sha256":artifact_sha256,"reference":reference,
        "policy_sha256":prepared.policy_sha256,"qualitygate_sha256":delivery::file_sha256(&operation.join("merged-qualitygate.json"))?}),
    )
}

fn regular(path: &Path, max: u64) -> Result<Vec<u8>> {
    let meta = fs::symlink_metadata(path)?;
    ensure!(
        meta.is_file() && meta.len() <= max,
        "activation input must be a bounded regular file"
    );
    Ok(fs::read(path)?)
}
fn relative(path: &Path) -> Result<()> {
    ensure!(
        !path.as_os_str().is_empty()
            && path
                .components()
                .all(|part| matches!(part, Component::Normal(_))),
        "activation artifact path must be relative without traversal"
    );
    Ok(())
}
fn unit(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.len() <= 100
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.')),
        "invalid activation unit name"
    );
    Ok(())
}
fn command(args: &[String], cwd: &Path, seconds: u64) -> Result<Vec<u8>> {
    let (program, args) = args.split_first().context("activation command is empty")?;
    let output = process::capture(program, args, cwd, Duration::from_secs(seconds))?;
    ensure!(
        output.status.success(),
        "activation command failed: {}",
        String::from_utf8_lossy(&output.stderr)
            .chars()
            .take(1200)
            .collect::<String>()
    );
    Ok(output.stdout)
}
fn text(args: &[String], cwd: &Path) -> Result<String> {
    Ok(String::from_utf8(command(args, cwd, 120)?)?
        .trim()
        .to_owned())
}
fn systemctl(args: &[String]) -> Result<Vec<u8>> {
    command(
        &[vec!["systemctl".into(), "--user".into()], args.to_vec()].concat(),
        Path::new("/"),
        120,
    )
}
fn write_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .context("activation destination lacks parent")?;
    fs::create_dir_all(parent)?;
    let temporary = path.with_extension(format!("activation-{}", std::process::id()));
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::rename(&temporary, path)?;
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}
fn phase(operation: &Path, phase: ActivationPhase, extra: Value) -> Result<()> {
    storage::write(
        &operation.join("status.json"),
        &json!({"phase":phase,"observed_ms":collaboration_model::now_ms(),"details":extra}),
    )
}

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

fn copy_tree(source: &Path, target: &Path, count: &mut usize, bytes: &mut u64) -> Result<()> {
    fs::create_dir_all(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        *count += 1;
        ensure!(*count <= 16384, "activation package file limit exceeded");
        let metadata = fs::symlink_metadata(entry.path())?;
        ensure!(
            !metadata.file_type().is_symlink(),
            "activation package cannot contain symlinks"
        );
        let destination = target.join(entry.file_name());
        if metadata.is_dir() {
            copy_tree(&entry.path(), &destination, count, bytes)?;
        } else {
            ensure!(
                metadata.is_file(),
                "activation package must contain regular files"
            );
            *bytes += metadata.len();
            ensure!(
                *bytes <= 512 * 1024 * 1024,
                "activation package byte limit exceeded"
            );
            fs::copy(entry.path(), destination)?;
        }
    }
    Ok(())
}
fn immutable(path: &Path) -> Result<()> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            immutable(&entry.path())?;
        } else {
            let mode = if entry.metadata()?.permissions().mode() & 0o111 != 0 {
                0o555
            } else {
                0o444
            };
            fs::set_permissions(entry.path(), fs::Permissions::from_mode(mode))?;
        }
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o555))?;
    Ok(())
}
fn clean(source: &Path, expected: &str) -> Result<()> {
    ensure!(
        text(&["git".into(), "rev-parse".into(), "HEAD".into()], source)? == expected
            && text(
                &["git".into(), "status".into(), "--porcelain".into()],
                source
            )?
            .is_empty(),
        "activation build modified the verified source snapshot"
    );
    Ok(())
}
fn build(operation: &Path, prepared: &Prepared, config: &Config) -> Result<(PathBuf, Value)> {
    phase(operation, ActivationPhase::Building, json!({}))?;
    let request = &prepared.request;
    let repository = &prepared.repository;
    let remote = freshness::RemoteClient::default().remote_snapshot(&repository.checkout)?;
    ensure!(
        remote.repository == request.repository && remote.commit == request.merged_commit,
        "merged activation source is no longer the upstream default"
    );
    command(
        &[
            "git".into(),
            "fetch".into(),
            "origin".into(),
            request.merged_commit.clone(),
            request.candidate_commit.clone(),
        ],
        &repository.checkout,
        120,
    )?;
    let source = operation.join("source");
    ensure!(
        !source.exists(),
        "activation source already exists; reconcile the interrupted operation"
    );
    command(
        &[
            "git".into(),
            "worktree".into(),
            "add".into(),
            "--detach".into(),
            source.display().to_string(),
            request.merged_commit.clone(),
        ],
        &repository.checkout,
        120,
    )?;
    let tree = text(
        &[
            "git".into(),
            "rev-parse".into(),
            format!("{}^{{tree}}", request.merged_commit),
        ],
        &source,
    )?;
    ensure!(
        tree == text(
            &[
                "git".into(),
                "rev-parse".into(),
                format!("{}^{{tree}}", request.candidate_commit)
            ],
            &source
        )?,
        "merged tree differs from the independently verified candidate"
    );
    for (index, args) in repository.build_commands.iter().enumerate() {
        fs::write(
            operation.join(format!("build-{index}.log")),
            command(args, &source, 1800)?,
        )?;
    }
    clean(&source, &request.merged_commit)?;
    let qg = config
        .tools
        .get("qualitygate-cli")
        .context("Qualitygate unavailable")?
        .binary
        .display()
        .to_string();
    let report: Value = serde_json::from_slice(&command(
        &repository.qualitygate_command(qg, &source, &request.merged_commit),
        &source,
        1800,
    )?)?;
    validate_gate(&report, &request.merged_commit)?;
    clean(&source, &request.merged_commit)?;
    storage::write(&operation.join("merged-qualitygate.json"), &report)?;
    freshness::RemoteClient::default().verify_remote_binding(&remote)?;
    Ok((source, report))
}
fn validate_gate(report: &Value, commit: &str) -> Result<()> {
    ensure!(
        report["profile"] == "full"
            && matches!(report["scope"].as_str(), Some("delivery" | "task"))
            && report["gate"]["complete"] == true
            && report["gate"]["decision"] == contracts::VerificationVerdict::Pass.as_str()
            && report["selection"]["empty_delivery"] == false
            && report["snapshot"]["mode"] == "diff"
            && report["snapshot"]["head"] == commit
            && report["plan"]["pending_delivery_checks"]
                .as_array()
                .is_some_and(Vec::is_empty),
        "activation requires an exact merged-commit complete nonempty full gate"
    );
    Ok(())
}
fn native_idle(config: &Config) -> Result<bool> {
    let directory = config.state_dir.join("jobs");
    if !directory.exists() {
        return Ok(true);
    }
    for (index, entry) in fs::read_dir(directory)?.enumerate() {
        ensure!(index < 16384, "activation job scan exceeds bound");
        let entry = entry?;
        if entry
            .path()
            .extension()
            .is_some_and(|value| value == "json")
        {
            let job: task::Job = storage::read(&entry.path())?;
            if job.launch.is_some() {
                return Ok(false);
            }
        }
    }
    Ok(true)
}
fn drain(operation: &Path, prepared: &Prepared, config: &Config) -> Result<storage::LockGuard> {
    phase(operation, ActivationPhase::Draining, json!({}))?;
    storage::write(
        &prepared.state_dir.join("private/activation-drain"),
        &json!({"operation":operation,"goal_id":prepared.request.goal_id}),
    )?;
    let started = Instant::now();
    loop {
        if let Ok(guard) = storage::lock(&config.state_dir.join("private/software-admission.lock"))
            && native_idle(config)?
        {
            return Ok(guard);
        }
        ensure!(
            started.elapsed() < Duration::from_secs(3600),
            "activation drain deadline exceeded"
        );
        std::thread::sleep(Duration::from_secs(2));
    }
}
fn service_health(policy: &Policy) -> Result<()> {
    let started = Instant::now();
    loop {
        let active = systemctl(&["is-active".into(), policy.main_unit.clone()]).is_ok();
        let http = http_health(&policy.health_url).is_ok();
        if active && http {
            return Ok(());
        }
        ensure!(
            started.elapsed() < Duration::from_secs(60),
            "activated service did not become healthy"
        );
        std::thread::sleep(Duration::from_secs(1));
    }
}

fn http_health(url: &str) -> Result<()> {
    let location = url
        .strip_prefix("http://")
        .context("health URL must use HTTP")?;
    let (authority, path) = location.split_once('/').unwrap_or((location, ""));
    let address: SocketAddr = authority
        .parse()
        .context("health URL requires an explicit IP and port")?;
    ensure!(!path.contains(['\r', '\n']), "invalid health path");
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(2))?;
    stream.set_read_timeout(Some(Duration::from_secs(3)))?;
    stream.set_write_timeout(Some(Duration::from_secs(3)))?;
    write!(
        stream,
        "GET /{path} HTTP/1.1\r\nHost: {authority}\r\nConnection: close\r\n\r\n"
    )?;
    let mut bytes = Vec::new();
    stream.take(1024 * 1024 + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 1024 * 1024, "health response exceeds bound");
    let status = std::str::from_utf8(
        bytes
            .split(|byte| *byte == b'\n')
            .next()
            .unwrap_or_default(),
    )?;
    let code = status
        .split_whitespace()
        .nth(1)
        .context("health response status missing")?
        .parse::<u16>()?;
    ensure!(
        (200..300).contains(&code),
        "health endpoint returned a non-success response"
    );
    Ok(())
}
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

fn executable(unit_text: &str) -> Result<&str> {
    let mut entries = unit_text
        .lines()
        .filter_map(|line| line.strip_prefix("ExecStart="));
    let value = entries.next().context("service unit lacks ExecStart")?;
    ensure!(
        entries.next().is_none(),
        "activation requires one explicit ExecStart"
    );
    let binary = value
        .split_whitespace()
        .next()
        .context("empty service command")?;
    ensure!(
        binary.starts_with('/') && !binary.contains(['\'', '"']),
        "activation requires an absolute unquoted service binary"
    );
    Ok(binary)
}
fn config_binding(unit_text: &str, expected: &Path) -> Result<()> {
    executable(unit_text)?;
    let line = unit_text
        .lines()
        .find_map(|line| line.strip_prefix("ExecStart="))
        .unwrap();
    let arguments: Vec<_> = line.split_whitespace().collect();
    let path = arguments
        .windows(2)
        .find_map(|pair| (pair[0] == "--config").then_some(pair[1]))
        .or_else(|| {
            arguments
                .iter()
                .find_map(|value| value.strip_prefix("--config="))
        })
        .context("service unit does not select the prepared config")?;
    ensure!(
        fs::canonicalize(path)? == fs::canonicalize(expected)?,
        "service unit selects a different config"
    );
    Ok(())
}
fn replace_executable(unit_text: &str, binary: &Path) -> Result<String> {
    let old = executable(unit_text)?;
    ensure!(
        !binary
            .as_os_str()
            .as_encoded_bytes()
            .iter()
            .any(u8::is_ascii_whitespace),
        "service artifact path contains whitespace"
    );
    Ok(unit_text.replacen(
        &format!("ExecStart={old}"),
        &format!("ExecStart={}", binary.display()),
        1,
    ))
}
fn platform_start(prepared: &Prepared) -> Result<()> {
    for (index, unit) in prepared.repository.platform_units.iter().enumerate() {
        systemctl(&["start".into(), unit.unit.clone()])?;
        if index == 0 {
            let socket = prepared
                .policy
                .storage_socket
                .as_ref()
                .context("platform storage socket required")?;
            let client = im_client::Client::remote(socket.clone())?;
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
fn activate_platform(
    operation: &Path,
    prepared: &Prepared,
    config: &Config,
    source: &Path,
) -> Result<Value> {
    let units = &prepared.repository.platform_units;
    ensure!(
        units.len() == 4
            && units[0].unit == "ai-im-storage"
            && units[1].unit == "ai-im-executor"
            && units[2].unit == "ai-im-sandbox"
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

fn release_drain(operation: &Path, prepared: &Prepared) -> Result<()> {
    let drain = prepared.state_dir.join("private/activation-drain");
    if drain.exists()
        && storage::read::<Value>(&drain)?["operation"] == operation.display().to_string()
    {
        fs::remove_file(drain)?;
    }
    Ok(())
}

pub fn worker(operation: &Path) -> Result<Value> {
    let operation = fs::canonicalize(operation)?;
    let prepared: Prepared = storage::read(&operation.join("prepared.json"))?;
    let _lock = storage::lock(
        &prepared
            .state_dir
            .join("private/software-activation-worker.lock"),
    )?;
    let _deadline = process::deadline_scope(Duration::from_secs(7000));
    let result = activate(&operation, &prepared);
    match result {
        Ok(receipt) => {
            storage::write(&operation.join("receipt.json"), &receipt)?;
            phase(&operation, ActivationPhase::Active, json!({}))?;
            release_drain(&operation, &prepared)?;
            Ok(receipt)
        }
        Err(error) => {
            let current: Value = storage::read(&operation.join("status.json"))?;
            let rolled_back =
                ActivationPhase::from_value(&current["phase"]) == Some(ActivationPhase::RolledBack);
            if rolled_back
                || confirmed_claim(&operation, &prepared).is_ok_and(|claim| claim.is_none())
            {
                release_drain(&operation, &prepared)?;
            }
            if !rolled_back {
                phase(
                    &operation,
                    ActivationPhase::Blocked,
                    json!({"error":error.to_string()}),
                )?;
            }
            Err(error)
        }
    }
}
fn activate(operation: &Path, prepared: &Prepared) -> Result<Value> {
    ensure!(
        delivery::file_sha256(&prepared.policy.service_config)? == prepared.config_sha256,
        "service config changed before activation"
    );
    ensure!(
        delivery::file_sha256(&prepared.state_dir.join("private/software-activation.json"))?
            == prepared.policy_sha256,
        "activation policy changed before execution"
    );
    let config = Config::load(&prepared.policy.service_config)?;
    let unit_text = String::from_utf8(systemctl(&[
        "cat".into(),
        prepared.policy.main_unit.clone(),
    ])?)?;
    config_binding(&unit_text, &prepared.policy.service_config)?;
    let (source, _) = build(operation, prepared, &config)?;
    if !prepared.repository.platform_units.is_empty() {
        return activate_platform(operation, prepared, &config, &source);
    }
    let (next, binary) = prepare_cli(operation, prepared, &config, &source)?;
    let _admission = drain(operation, prepared, &config)?;
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

fn verify_installation(prepared: &Prepared, proof: &SwitchProof, active: bool) -> Result<()> {
    let expected_config = if active {
        &proof.next_config_sha256
    } else {
        &proof.previous_config_sha256
    };
    ensure!(
        delivery::file_sha256(&prepared.policy.service_config)? == *expected_config,
        "installed configuration does not match activation proof"
    );
    Config::load(&prepared.policy.service_config)?;
    let unit_text = String::from_utf8(systemctl(&[
        "cat".into(),
        prepared.policy.main_unit.clone(),
    ])?)?;
    config_binding(&unit_text, &prepared.policy.service_config)?;
    if proof.units.is_empty() {
        ensure!(
            running_artifact(&prepared.policy.main_unit)? == proof.main_artifact_sha256,
            "running main service differs from the activation proof"
        );
        config_loaded_after_write(&prepared.policy)?;
        if active {
            ensure!(
                delivery::file_sha256(&proof.artifact)? == proof.artifact_sha256,
                "installed CLI artifact differs from activation proof"
            );
            command(
                &[
                    vec![proof.artifact.display().to_string()],
                    prepared.repository.probe.clone(),
                ]
                .concat(),
                proof
                    .artifact
                    .parent()
                    .context("artifact directory missing")?,
                30,
            )?;
        }
    } else {
        ensure!(
            proof.units.len() == 4,
            "platform proof must cover all services"
        );
        for item in &proof.units {
            let expected_unit = if active {
                &item.next_unit_sha256
            } else {
                &item.previous_unit_sha256
            };
            let expected_binary = if active {
                &item.next_artifact_sha256
            } else {
                &item.previous_artifact_sha256
            };
            ensure!(
                delivery::file_sha256(&item.unit_file)? == *expected_unit
                    && running_artifact(&item.unit)? == *expected_binary,
                "platform installation differs from activation proof"
            );
            systemctl(&["is-active".into(), item.unit.clone()])?;
        }
        let client = storage_client(prepared)?;
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(client.probe())?;
    }
    service_health(&prepared.policy)
}

/// Inspect exact installation proofs without replaying any deployment side effects.
pub fn reconcile(operation: &Path) -> Result<Value> {
    let operation = fs::canonicalize(operation)?;
    let prepared: Prepared = storage::read(&operation.join("prepared.json"))?;
    let _lock = storage::lock(
        &prepared
            .state_dir
            .join("private/software-activation-worker.lock"),
    )?;
    let _deadline = process::deadline_scope(Duration::from_secs(300));
    let Some(claim) = confirmed_claim(&operation, &prepared)? else {
        release_drain(&operation, &prepared)?;
        return Ok(
            json!({"schema_version":1,"goal_id":prepared.request.goal_id,
            "candidate_commit":prepared.request.candidate_commit,"merged_commit":prepared.request.merged_commit,
            "active":false,"phase":ActivationPhase::Blocked,"requires_reconciliation":false,
            "details":{"error":"Activation admission was rejected before any switch began."}}),
        );
    };
    let client = storage_client(&prepared)?;
    let run = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(client.evolution_run(&prepared.request.iteration_id))?;
    let round = run.rounds.last().context("iteration round missing")?;
    ensure!(
        run.state == IterationState::Running
            && round.goal_id == prepared.request.goal_id
            && round
                .activation
                .as_ref()
                .is_some_and(|activation| activation.state == ActivationState::Pending
                    && activation.reference == claim),
        "reconciliation no longer owns the pending activation"
    );
    let proof: SwitchProof = storage::read(&operation.join("switch-proof.json"))?;
    let active = match verify_installation(&prepared, &proof, true) {
        Ok(()) => true,
        Err(active_error) => {
            verify_installation(&prepared, &proof, false).with_context(||
                format!("neither candidate nor baseline installation can be verified; candidate: {active_error}"))?;
            false
        }
    };
    let receipt = bound_receipt(
        &operation,
        &prepared,
        active,
        &proof.artifact,
        &proof.artifact_sha256,
    )?;
    storage::write(&operation.join("receipt.json"), &receipt)?;
    phase(
        &operation,
        if active {
            ActivationPhase::Active
        } else {
            ActivationPhase::RolledBack
        },
        json!({"reconciled":true}),
    )?;
    release_drain(&operation, &prepared)?;
    Ok(receipt)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn merged_snapshot_budget_preserves_legacy_policy_and_full_gate_arguments() {
        let legacy = r#"{"schema_version":1,"service_config":"/config.toml","main_unit":"main.service","health_url":"http://127.0.0.1:8090/","repositories":{"owner/repo":{"checkout":"/repo","build_commands":[["cargo","build","--release"]],"package_directory":"dist/skill","binary_relative_path":"assets/linux/tool","probe":["--help"],"platform_units":[]}},"storage_state_dir":null,"storage_socket":"/storage.sock"}"#;
        let policy: Policy = serde_json::from_str(legacy).unwrap();
        let bytes = serde_json::to_vec(&policy).unwrap();
        assert_eq!(bytes, legacy.as_bytes());
        assert_eq!(
            storage::digest(&bytes),
            "51a4a95ee6a1bd02493a3d442daa4bceb001e87ce4a70b9a6b23319d613012b2"
        );
        let expected = [
            "qualitygate",
            "check",
            "--root",
            "/merged",
            "--diff",
            "merged^..merged",
            "--profile",
            "full",
            "--format",
            "json",
        ]
        .map(str::to_owned)
        .to_vec();
        assert_eq!(
            policy.repositories["owner/repo"].qualitygate_command(
                "qualitygate".into(),
                Path::new("/merged"),
                "merged",
            ),
            expected
        );
        for budget in [1, 5, 8] {
            let mut value: Value = serde_json::from_str(legacy).unwrap();
            value["repositories"]["owner/repo"]["qualitygate_snapshot_max_file_mib"] =
                json!(budget);
            let configured: Policy = serde_json::from_value(value).unwrap();
            let args = configured.repositories["owner/repo"].qualitygate_command(
                "qualitygate".into(),
                Path::new("/merged"),
                "merged",
            );
            assert_eq!(args[..expected.len()], expected);
            assert_eq!(
                args[expected.len()..],
                ["--snapshot-max-file-mib".to_owned(), budget.to_string()]
            );
            assert_ne!(
                storage::digest(&serde_json::to_vec(&configured).unwrap()),
                storage::digest(&bytes)
            );
            assert_eq!(
                serde_json::to_value(&configured).unwrap()["repositories"]["owner/repo"]["qualitygate_snapshot_max_file_mib"],
                budget
            );
        }
        for invalid in [
            json!(0),
            json!(9),
            json!(-1),
            json!(256),
            json!(1.5),
            json!("5"),
            Value::Null,
        ] {
            let mut value: Value = serde_json::from_str(legacy).unwrap();
            value["repositories"]["owner/repo"]["qualitygate_snapshot_max_file_mib"] = invalid;
            assert!(serde_json::from_value::<Policy>(value).is_err());
        }
    }
    #[test]
    fn rejected_admission_does_not_leave_a_phantom_pending_activation() {
        let mut round = collaboration_model::EvolutionRound {
            number: 1,
            state: RoundState::Accepted,
            goal_id: "goal".into(),
            goal_attempts: BTreeMap::new(),
            baseline_version_id: "baseline".into(),
            started_ms: 1,
            finished_ms: None,
            feedback: String::new(),
            candidate: None,
            evaluation: None,
            activation: None,
        };
        assert_eq!(confirmed_round_claim(&round, "claim", false).unwrap(), None);
        assert!(confirmed_round_claim(&round, "claim", true).is_err());
        round.activation = Some(collaboration_model::CandidateActivation {
            state: ActivationState::Pending,
            reference: "claim".into(),
            evidence: json!({"activation_claim_id":"claim"}),
        });
        assert_eq!(
            confirmed_round_claim(&round, "claim", false).unwrap(),
            Some("claim".into())
        );
        assert!(confirmed_round_claim(&round, "different-claim", false).is_err());
    }
    #[test]
    fn activation_rejects_empty_failed_or_stale_full_gate() {
        let commit = "a".repeat(40);
        let valid = json!({"profile":"full","scope":"delivery","gate":{"complete":true,"decision":"pass"},"selection":{"empty_delivery":false},"snapshot":{"mode":"diff","head":commit},"plan":{"pending_delivery_checks":[]}});
        validate_gate(&valid, &commit).unwrap();
        for (pointer, value) in [
            ("/selection/empty_delivery", json!(true)),
            ("/snapshot/head", json!("b".repeat(40))),
            ("/gate/complete", json!(false)),
            ("/plan/pending_delivery_checks", json!(["test"])),
        ] {
            let mut bad = valid.clone();
            *bad.pointer_mut(pointer).unwrap() = value;
            assert!(validate_gate(&bad, &commit).is_err());
        }
    }
    #[test]
    fn artifact_paths_and_service_units_cannot_escape_host_configuration() {
        for value in ["../binary", "/bin/tool", "assets/../tool", ""] {
            assert!(relative(Path::new(value)).is_err());
        }
        relative(Path::new("assets/linux/tool")).unwrap();
        for value in ["../service", "service\nExecStart=evil", "service;evil", ""] {
            assert!(unit(value).is_err());
        }
        unit("agent-research-lab.service").unwrap();
    }
    #[test]
    fn platform_binary_replacement_preserves_endpoints_and_environment() {
        let previous = "[Service]\nEnvironmentFile=/private/service.env\nExecStart=/old/ai-im-storage --state-dir /durable/store --socket /private/storage.sock --max-seconds 43200\nRestart=always\n";
        let changed = replace_executable(previous, Path::new("/new/ai-im-storage")).unwrap();
        assert!(changed.contains("EnvironmentFile=/private/service.env"));
        assert!(changed.contains("ExecStart=/new/ai-im-storage --state-dir /durable/store --socket /private/storage.sock --max-seconds 43200"));
        assert!(replace_executable("ExecStart=/one\nExecStart=/two", Path::new("/new")).is_err());
    }
    #[test]
    fn health_checks_use_actual_http_status_and_exact_host() {
        for (status, succeeds) in [
            ("200 OK", true),
            ("401 Unauthorized", false),
            ("503 Unavailable", false),
        ] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut request = Vec::new();
                while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
                    let mut buffer = [0u8; 256];
                    let count = stream.read(&mut buffer).unwrap();
                    assert!(count > 0 && request.len() + count <= 1024);
                    request.extend_from_slice(&buffer[..count]);
                }
                assert!(
                    String::from_utf8_lossy(&request).contains(&format!("Host: {address}\r\n"))
                );
                write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                )
                .unwrap();
            });
            assert_eq!(http_health(&format!("http://{address}/")).is_ok(), succeeds);
            server.join().unwrap();
        }
    }
}
