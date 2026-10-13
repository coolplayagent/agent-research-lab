//! Host-owned software evidence and delivery. Native workers cannot authorize effects.
use super::*;
use contracts::{CandidateState, CodingTaskRole, EvaluationState, IterationState, RoundState};
use crystal::{EvolutionAction, EvolutionChange, EvolutionRun, Goal, WorkState};
use std::{
    path::PathBuf,
    sync::{Mutex, OnceLock},
};

#[derive(Clone, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct DeliveryPolicy {
    schema_version: u32,
    repositories: BTreeMap<String, RepositoryPolicy>,
}
#[derive(Clone, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct RepositoryPolicy {
    repository: String,
    tests: Vec<Vec<String>>,
    /// Host-owned deployment adapter; reads one JSON request path, returns a bound receipt.
    activation: Vec<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::software_verification::optional_budget"
    )]
    qualitygate_snapshot_max_file_mib: Option<crate::software_verification::SnapshotMaxFileMib>,
}
impl RepositoryPolicy {
    fn qualitygate_command(
        &self,
        program: String,
        source: &Path,
        baseline: &str,
        candidate: &str,
    ) -> Vec<String> {
        crate::software_verification::qualitygate_command(
            program,
            source,
            format!("{baseline}..{candidate}"),
            self.qualitygate_snapshot_max_file_mib,
        )
    }
}
#[derive(serde::Serialize, serde::Deserialize)]
struct Verified {
    implementer: Job,
    reviewer: Job,
    evidence: delivery::ReleaseEvidence,
    policy: RepositoryPolicy,
    candidate: String,
    directory: PathBuf,
}
static RUNNING: OnceLock<Mutex<BTreeSet<String>>> = OnceLock::new();
pub(super) fn policy_digest(c: &Config) -> Result<String> {
    let path = c.state_dir.join("private/software-delivery.json");
    ensure!(
        fs::metadata(&path)?.len() <= 65536,
        "software delivery policy exceeds bound"
    );
    let policy: DeliveryPolicy = storage::read(&path)?;
    ensure!(
        policy.schema_version == 1,
        "unsupported software delivery policy"
    );
    Ok(storage::digest(&serde_json::to_vec(&policy)?))
}
pub(super) fn enabled_repositories(c: &Config) -> Result<Vec<String>> {
    let path = c.state_dir.join("private/software-delivery.json");
    ensure!(
        fs::metadata(&path)?.len() <= 65536,
        "software delivery policy exceeds bound"
    );
    let policy: DeliveryPolicy = storage::read(&path)?;
    ensure!(
        policy.schema_version == 1,
        "unsupported software delivery policy"
    );
    Ok(policy
        .repositories
        .into_iter()
        .filter(|(name, policy)| {
            (name == "agent-research-lab" || c.tools.contains_key(name))
                && !policy.tests.is_empty()
                && !policy.activation.is_empty()
        })
        .map(|(name, _)| name)
        .collect())
}
struct Running(String);
impl Drop for Running {
    fn drop(&mut self) {
        RUNNING.get().unwrap().lock().unwrap().remove(&self.0);
    }
}
fn artifact(path: &Path, value: &Value) -> Result<delivery::VerificationArtifact> {
    storage::write(path, value)?;
    Ok(delivery::VerificationArtifact {
        path: path.into(),
        sha256: delivery::file_sha256(path)?,
    })
}
fn command(args: &[String], cwd: &Path, seconds: u64) -> Result<Vec<u8>> {
    let (program, args) = args.split_first().context("empty host command")?;
    let result = process::capture(program, args, cwd, Duration::from_secs(seconds))?;
    ensure!(
        result.status.success(),
        "host command failed: {}",
        String::from_utf8_lossy(&result.stderr)
            .chars()
            .take(2000)
            .collect::<String>()
    );
    Ok(result.stdout)
}
fn clean_candidate(job: &Job, candidate: &str) -> Result<()> {
    ensure!(
        delivery::capture_text("git", &["rev-parse".into(), "HEAD".into()], &job.worktree)?
            == candidate,
        "candidate changed during verification"
    );
    ensure!(
        delivery::capture_text(
            "git",
            &["status".into(), "--porcelain".into()],
            &job.worktree
        )?
        .is_empty(),
        "verification modified candidate files"
    );
    Ok(())
}
fn verify(
    c: &Config,
    run: &EvolutionRun,
    goal: &Goal,
    baseline_source: &str,
) -> Result<Option<Verified>> {
    let _deadline = process::deadline_scope(Duration::from_secs(1800));
    let parameters: super::software_scenario::Parameters =
        serde_json::from_value(run.input.parameters.clone())?;
    let directory = c
        .state_dir
        .join("private/software-delivery")
        .join(&run.input.id)
        .join(goal.input.id.clone());
    fs::create_dir_all(&directory)?;
    let policy_path = c.state_dir.join("private/software-delivery.json");
    ensure!(
        fs::metadata(&policy_path)?.len() <= 65536,
        "software delivery policy exceeds bound"
    );
    let policy: DeliveryPolicy = storage::read(&policy_path)?;
    ensure!(
        policy.schema_version == 1,
        "unsupported software delivery policy"
    );
    let policy_pin = storage::digest(&serde_json::to_vec(&policy)?);
    ensure!(
        parameters.delivery_policy_digest == policy_pin,
        "delivery policy differs from frozen goal"
    );
    let pin_path = directory.join("policy.json");
    if pin_path.exists() {
        ensure!(
            storage::read::<Value>(&pin_path)?["digest"] == policy_pin,
            "delivery policy changed during round"
        );
    } else {
        storage::write(&pin_path, &json!({"digest":policy_pin}))?;
    }
    let policy = policy
        .repositories
        .get(&parameters.repository)
        .context("software delivery repository is not enabled")?
        .clone();
    ensure!(
        !policy.tests.is_empty() && !policy.activation.is_empty(),
        "tests and activation adapter are required"
    );
    let job = |role| -> Result<Job> {
        let work = goal
            .work
            .iter()
            .find(|w| parameters.roles.get(&w.person_id) == Some(&role))
            .context("software role missing")?;
        ensure!(
            work.state == WorkState::Submitted && work.result.as_ref().is_some_and(|r| r.succeeded),
            "software role did not submit a successful execution"
        );
        let id = super::research_scenario::job_id(goal, work);
        let job = read_job(&c.state_dir.join("jobs").join(format!("{id}.json")))?;
        ensure!(
            job.task.id == id
                && job.task.persona_id.as_deref() == Some(work.person_id.as_str())
                && job.attempt == 1,
            "software job attempt mismatch"
        );
        Ok(job)
    };
    let implementer = job(CodingTaskRole::Implement)?;
    let reviewer = job(CodingTaskRole::Review)?;
    ensure!(
        implementer.source_commit == baseline_source,
        "upstream changed from the iteration baseline; a new baseline assessment is required"
    );
    let produced = runtime::completed_result(c, &implementer)?;
    let reviewed = runtime::completed_result(c, &reviewer)?;
    let candidate = produced["candidate"]["candidate_commit"]
        .as_str()
        .context("missing host captured candidate")?
        .to_owned();
    if produced["candidate"]["no_change"] == true {
        return Ok(None);
    }
    ensure!(
        reviewer.source_commit == candidate
            && reviewer.baseline_commit.as_deref() == Some(implementer.source_commit.as_str()),
        "review did not execute against the candidate"
    );
    let findings = reviewed["agent_report"]["findings"]
        .as_array()
        .context("review findings missing")?;
    ensure!(
        findings
            .iter()
            .any(|v| v.as_str() == Some("review_verdict=pass"))
            && !findings.iter().any(|v| v
                .as_str()
                .is_some_and(|v| v.contains("review_verdict=fail"))),
        "independent reviewer did not approve the candidate"
    );
    ensure!(
        delivery::capture_text(
            "git",
            &["rev-parse".into(), "HEAD".into()],
            &implementer.worktree
        )? == candidate,
        "candidate changed before verification"
    );
    ensure!(
        delivery::capture_text(
            "git",
            &["status".into(), "--porcelain".into()],
            &implementer.worktree
        )?
        .is_empty(),
        "candidate worktree is dirty"
    );
    let verified_path = directory.join("verified.json");
    if verified_path.exists() {
        let verified: Verified = storage::read(&verified_path)?;
        ensure!(
            verified.candidate == candidate
                && verified.implementer.run_id == implementer.run_id
                && verified.reviewer.run_id == reviewer.run_id
                && verified.reviewer.prompt_digest == reviewer.prompt_digest,
            "cached verification belongs to another attempt"
        );
        verified.evidence.validate()?;
        return Ok(Some(verified));
    }
    let upstream = implementer
        .research_inputs
        .as_ref()
        .context("candidate lacks freshness evidence")?
        .repositories
        .get(&parameters.repository)
        .context("candidate repository evidence missing")?
        .upstream
        .clone();
    ensure!(
        upstream.repository == policy.repository,
        "delivery repository differs from frozen research source"
    );
    freshness::RemoteClient::default().verify_remote_binding(&upstream)?;
    for (index, test) in policy.tests.iter().enumerate() {
        let output = command(test, &implementer.worktree, 900)?;
        fs::write(directory.join(format!("test-{index}.log")), output)?;
    }
    let qg = c
        .tools
        .get("qualitygate-cli")
        .context("Qualitygate is not configured")?
        .binary
        .to_string_lossy()
        .into_owned();
    if !implementer.worktree.join("qualitygate.yaml").exists()
        && !implementer.worktree.join("qualitygate.yml").exists()
    {
        command(
            &[
                qg.clone(),
                "init".into(),
                "--root".into(),
                implementer.worktree.display().to_string(),
            ],
            &implementer.worktree,
            60,
        )?;
        anyhow::bail!("new Qualitygate policy needs adoption before candidate delivery");
    }
    let args = policy.qualitygate_command(
        qg,
        &implementer.worktree,
        &implementer.source_commit,
        &candidate,
    );
    let report: Value = serde_json::from_slice(&command(&args, &implementer.worktree, 1200)?)?;
    clean_candidate(&implementer, &candidate)?;
    ensure!(
        report["selection"]["empty_delivery"] != true,
        "empty delivery is not candidate verification"
    );
    let qualitygate_report = artifact(&directory.join("qualitygate.json"), &report)?;
    let skills_path = c
        .skills_manifest
        .as_ref()
        .context("skills manifest required")?;
    let skills_manifest = artifact(
        &directory.join("skills.json"),
        &storage::read::<Value>(skills_path)?,
    )?;
    let bindings = json!({"source_commit":candidate,"upstream":upstream,"skills_manifest_sha256":skills_manifest.sha256,"prompt_sha256":implementer.prompt_digest,"policy_sha256":implementer.config_digest,"superpod_commit":implementer.superpod_commit});
    let mut snapshot = bindings.clone();
    snapshot["qualitygate_report_sha256"] = json!(qualitygate_report.sha256);
    snapshot["content_digest"] = report["snapshot"]["content_digest"].clone();
    snapshot["verification_digest"] = report["snapshot"]["verification_digest"].clone();
    let snapshot_receipt = artifact(&directory.join("snapshot.json"), &snapshot)?;
    let mut evaluation = bindings;
    evaluation["role"] = json!("independent_evaluator");
    evaluation["status"] = json!(contracts::VerificationVerdict::Pass);
    evaluation["complete"] = json!(true);
    evaluation["implementer_id"] = json!(implementer.task.persona_id);
    evaluation["evaluator_id"] = json!(reviewer.task.persona_id);
    evaluation["review_receipt_sha256"] = json!(storage::digest(&serde_json::to_vec(&reviewed)?));
    let evaluation_receipt = artifact(&directory.join("evaluation.json"), &evaluation)?;
    let evidence = delivery::ReleaseEvidence {
        source_commit: candidate.clone(),
        upstream: Some(upstream),
        skills_manifest: Some(skills_manifest),
        prompt_sha256: implementer.prompt_digest.clone(),
        policy_sha256: implementer.config_digest.clone(),
        superpod_commit: implementer.superpod_commit.clone(),
        implementer_id: implementer.task.persona_id.clone().unwrap(),
        evaluator_id: reviewer.task.persona_id.clone().unwrap(),
        evaluation_passed: true,
        qualitygate_passed: true,
        qualitygate_report,
        snapshot_receipt,
        evaluation_receipt,
    };
    evidence.validate()?;
    let verified = Verified {
        implementer,
        reviewer,
        evidence,
        policy,
        candidate,
        directory,
    };
    storage::write(&verified_path, &verified)?;
    Ok(Some(verified))
}
fn activation_output(bytes: &[u8]) -> Result<Value> {
    let mut value: Value = serde_json::from_slice(bytes)?;
    if value.get("ok").is_some() {
        ensure!(
            value.as_object().is_some_and(|object| object
                .keys()
                .all(|key| matches!(key.as_str(), "ok" | "result"))),
            "activation command envelope contains conflicting fields"
        );
        ensure!(value["ok"] == true, "activation command reported failure");
        value = value
            .get_mut("result")
            .context("activation command result missing")?
            .take();
    }
    ensure!(
        value.is_object() && value["schema_version"] == 1,
        "activation adapter returned an unsupported receipt"
    );
    Ok(value)
}
fn validate_activation(
    activated: &Value,
    candidate: &str,
    merged_commit: &str,
    goal_id: &str,
) -> Result<()> {
    ensure!(
        activated["schema_version"] == 1
            && activated["candidate_commit"] == candidate
            && activated["merged_commit"] == merged_commit
            && activated["goal_id"] == goal_id
            && activated["active"] == true
            && activated["artifact_sha256"]
                .as_str()
                .is_some_and(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
            && activated["reference"]
                .as_str()
                .is_some_and(|s| !s.is_empty()),
        "activation receipt is incomplete or does not bind this goal"
    );
    Ok(())
}
async fn change(
    hub: &im_storage::Client,
    run: &EvolutionRun,
    suffix: &str,
    action: EvolutionAction,
) -> Result<EvolutionRun> {
    hub.evolution_change(EvolutionChange {
        request_id: format!("host-{}-{suffix}", run.revision),
        run_id: run.input.id.clone(),
        revision: run.revision,
        action,
    })
    .await
}
async fn current(hub: &im_storage::Client, run: &EvolutionRun) -> Result<EvolutionRun> {
    let latest = hub.evolution_run(&run.input.id).await?;
    ensure!(
        latest.state == IterationState::Running
            && latest.rounds.last().map(|r| &r.goal_id) == run.rounds.last().map(|r| &r.goal_id),
        "iteration paused, stopped or advanced"
    );
    Ok(latest)
}
async fn finish(
    hub: &im_storage::Client,
    run: &EvolutionRun,
    state: RoundState,
    feedback: String,
) -> Result<()> {
    let latest = current(hub, run).await?;
    change(
        hub,
        &latest,
        "finish",
        EvolutionAction::FinishRound { state, feedback },
    )
    .await?;
    Ok(())
}
async fn deliver(
    hub: im_storage::Client,
    c: Arc<Config>,
    initial: EvolutionRun,
    goal: Goal,
) -> Result<()> {
    let baseline_id = &initial
        .rounds
        .last()
        .context("missing software round")?
        .baseline_version_id;
    let activated_source = initial.rounds.iter().rev().find_map(|round| {
        let candidate = round.candidate.as_ref()?;
        let activation = round.activation.as_ref()?;
        (candidate.version_id == *baseline_id
            && activation.state == contracts::ActivationState::Active)
            .then(|| {
                activation.evidence["merged_commit"]
                    .as_str()
                    .map(str::to_owned)
            })
            .flatten()
    });
    let baseline_source = if let Some(source) = activated_source {
        source
    } else {
        let mut after = 0;
        loop {
            let page = hub.graph("", &initial.input.subject_id, after).await?;
            let found = page["nodes"]
                .as_array()
                .context("invalid version graph")?
                .iter()
                .find(|row| row["node"]["input"]["id"] == *baseline_id);
            if let Some(row) = found {
                break row["node"]["input"]["pins"]["source"]
                    .as_str()
                    .context("software baseline requires a source commit")?
                    .to_owned();
            }
            match page["next_after"].as_u64() {
                Some(next) if next > after => after = next,
                _ => anyhow::bail!("software baseline is unavailable"),
            }
        }
    };
    let config = c.clone();
    let frozen = initial.clone();
    let checked_goal = goal.clone();
    let verified = tokio::task::spawn_blocking(move || {
        verify(&config, &frozen, &checked_goal, &baseline_source)
    })
    .await??;
    let Some(verified) = verified else {
        return finish(
            &hub,
            &initial,
            RoundState::NoImprovement,
            "No supported source change; retain baseline and explore a different hypothesis."
                .into(),
        )
        .await;
    };
    let parameters: super::software_scenario::Parameters =
        serde_json::from_value(initial.input.parameters.clone())?;
    let producer = goal
        .work
        .iter()
        .find(|w| parameters.roles[&w.person_id] == CodingTaskRole::Implement)
        .unwrap();
    let reviewer = goal
        .work
        .iter()
        .find(|w| parameters.roles[&w.person_id] == CodingTaskRole::Review)
        .unwrap();
    let version_id = format!(
        "candidate-{}",
        &storage::digest(format!("{}:{}", initial.input.subject_id, verified.candidate).as_bytes())
            [..40]
    );
    let mut run = current(&hub, &initial).await?;
    hub.research_change(crystal::ResearchChange::Node {
        node: crystal::EvolutionInput {
            id: version_id.clone(),
            subject_id: run.input.subject_id.clone(),
            parents: vec![run.rounds.last().unwrap().baseline_version_id.clone()],
            version: verified.candidate.clone(),
            reference: format!(
                "https://github.com/{}/commit/{}",
                verified.policy.repository, verified.candidate
            ),
            goal_id: Some(goal.input.id.clone()),
            pins: crystal::EvidencePins {
                source: Some(verified.candidate.clone()),
                prompt: Some(verified.reviewer.prompt_digest.clone()),
                policy: Some(verified.reviewer.config_digest.clone()),
                superpod: Some(verified.reviewer.superpod_commit.clone()),
            },
        },
    })
    .await?;
    if run.rounds.last().unwrap().candidate.is_none() {
        run = change(
            &hub,
            &run,
            "candidate",
            EvolutionAction::RecordCandidate {
                version_id: version_id.clone(),
                producer_work_id: producer.id.clone(),
            },
        )
        .await?;
    }
    if run.rounds.last().unwrap().evaluation.is_none() {
        run=change(&hub,&run,"evaluate",EvolutionAction::Evaluate {evaluation:crystal::CandidateEvaluation {evaluator_work_id:reviewer.id.clone(),goal_revision:goal.revision,state:EvaluationState::Passed,summary:"Independent candidate review and host repository checks passed.".into(),evidence:json!({"work_evidence":reviewer.result.as_ref().unwrap().evidence,"host_evidence":{"qualitygate_sha256":verified.evidence.qualitygate_report.sha256,"snapshot_sha256":verified.evidence.snapshot_receipt.sha256,"evaluation_sha256":verified.evidence.evaluation_receipt.sha256}})}}).await?;
    }
    if run.rounds.last().unwrap().candidate.as_ref().unwrap().state != CandidateState::Accepted {
        run = change(&hub, &run, "accept", EvolutionAction::Accept).await?;
    }
    let branch = format!("rsi/{}", &storage::digest(goal.input.id.as_bytes())[..32]);
    current(&hub, &run).await?;
    let policy = verified.policy.clone();
    let cwd = verified.implementer.worktree.clone();
    let expected = verified.candidate.clone();
    let evidence = verified.evidence.clone();
    let title = goal.input.title.clone();
    let evidence_dir = verified.directory.clone();
    let checked_job = verified.implementer.clone();
    let receipt=tokio::task::spawn_blocking(move|| -> Result<delivery::PullRequestReceipt> {
        let _deadline=process::deadline_scope(Duration::from_secs(180));
        clean_candidate(&checked_job, &expected)?;
        delivery::capture_text("git",&["push".into(),"origin".into(),format!("{expected}:refs/heads/{branch}")],&cwd)?;
        let pr=delivery::ensure_pull_request(&delivery::PullRequestRequest {repository:policy.repository.clone(),working_directory:cwd.clone(),branch,base:evidence.upstream.as_ref().unwrap().default_branch.clone(),expected_head:expected.clone(),title,body:format!("Implements a verified RSI candidate.\n\nValidation: repository tests, full Qualitygate, and an independent review of commit {expected}.\n\nEvidence snapshot: {}.",evidence.snapshot_receipt.sha256)})?;
        storage::write(&evidence_dir.join("pr.json"),&pr)?;
        Ok(pr)
    }).await??;
    hub.publish_as("operator",crystal::Publish {group_id:run.input.group_id.clone(),request_id:format!("software-pr-{}",goal.input.id),text:format!("Candidate {} passed repository checks and independent review. Merge request: {}. CI and merge checks remain required.",verified.candidate,receipt.url),reply_to:None}).await?;
    current(&hub, &run).await?;
    let merge = delivery::MergeRequest {
        repository: verified.policy.repository.clone(),
        working_directory: verified.implementer.worktree.clone(),
        number: receipt.number,
        expected_head: verified.candidate.clone(),
        evidence: verified.evidence.clone(),
    };
    let merged =
        tokio::task::spawn_blocking(move || delivery::merge_pull_request(&merge)).await??;
    ensure!(
        merged.state == contracts::PullRequestState::Merged.as_str(),
        "merge is pending; activation remains blocked"
    );
    hub.publish_as(
        "operator",
        crystal::Publish {
            group_id: run.input.group_id.clone(),
            request_id: format!("software-merged-{}", goal.input.id),
            text: format!(
                "Merge request {} is merged. Activation verification is pending.",
                merged.url
            ),
            reply_to: None,
        },
    )
    .await?;
    current(&hub, &run).await?;
    let request_path = verified.directory.join("activation-request.json");
    let merge_repository = verified.policy.repository.clone();
    let merge_number = merged.number;
    let merge_cwd = verified.implementer.worktree.clone();
    let candidate = verified.candidate.clone();
    let merged_commit = tokio::task::spawn_blocking(move || -> Result<String> {
        let value: Value = serde_json::from_str(&delivery::capture_text(
            "gh",
            &[
                "pr".into(),
                "view".into(),
                merge_number.to_string(),
                "--repo".into(),
                merge_repository,
                "--json".into(),
                "mergeCommit,headRefOid".into(),
            ],
            &merge_cwd,
        )?)?;
        ensure!(value["headRefOid"] == candidate, "merged head changed");
        let commit = value["mergeCommit"]["oid"]
            .as_str()
            .context("merged commit unavailable")?
            .to_owned();
        ensure!(
            commit.len() == 40 && commit.bytes().all(|b| b.is_ascii_hexdigit()),
            "invalid merged commit"
        );
        delivery::capture_text(
            "git",
            &["fetch".into(), "origin".into(), commit.clone()],
            &merge_cwd,
        )?;
        let tree = |id: &str| {
            delivery::capture_text(
                "git",
                &["rev-parse".into(), format!("{id}^{{tree}}")],
                &merge_cwd,
            )
        };
        ensure!(
            tree(&commit)? == tree(&candidate)?,
            "merged tree differs from verified candidate"
        );
        Ok(commit)
    })
    .await??;
    storage::write(
        &request_path,
        &json!({"schema_version":1,"iteration_id":run.input.id,"goal_id":goal.input.id,"candidate_commit":verified.candidate,"merged_commit":merged_commit,"repository":verified.policy.repository,"merge":merged,"evidence":verified.evidence}),
    )?;
    let mut args = verified.policy.activation.clone();
    args.push(request_path.display().to_string());
    let cwd = verified.implementer.worktree.clone();
    let activated_path = verified.directory.join("activated.json");
    let activated: Value = if activated_path.exists() {
        storage::read(&activated_path)?
    } else {
        activation_output(&tokio::task::spawn_blocking(move || command(&args, &cwd, 1800)).await??)?
    };
    ensure!(
        activated["requires_reconciliation"] != true,
        "activation outcome is uncertain; verify actual services and reconcile the durable host claim before continuing"
    );
    if let Some(
        phase @ (contracts::ActivationPhase::Blocked | contracts::ActivationPhase::RolledBack),
    ) = contracts::ActivationPhase::from_value(&activated["phase"])
    {
        ensure!(
            activated["goal_id"] == goal.input.id
                && activated["candidate_commit"] == verified.candidate
                && activated["merged_commit"] == merged_commit,
            "activation failure belongs to another operation"
        );
        let rolled_back = phase == contracts::ActivationPhase::RolledBack;
        let latest = current(&hub, &run).await?;
        change(
            &hub,
            &latest,
            "activation-failed",
            EvolutionAction::Activate {
                activation: crystal::CandidateActivation {
                    state: if rolled_back {
                        contracts::ActivationState::RolledBack
                    } else {
                        contracts::ActivationState::Failed
                    },
                    reference: format!("activation:{}", goal.input.id),
                    evidence: activated,
                },
            },
        )
        .await?;
        return finish(&hub, &run, RoundState::Failed, if rolled_back {
            "Activation failed and the previous installation was restored. Inspect host activation evidence before retrying."
        } else {
            "Activation is blocked. The baseline was not promoted; inspect host evidence and verify the actual installation state before retrying."
        }.into()).await;
    }
    validate_activation(
        &activated,
        &verified.candidate,
        &merged_commit,
        &goal.input.id,
    )?;
    if !activated_path.exists() {
        storage::write(&activated_path, &activated)?;
    }
    let latest = current(&hub, &run).await?;
    change(
        &hub,
        &latest,
        "activate",
        EvolutionAction::Activate {
            activation: crystal::CandidateActivation {
                state: contracts::ActivationState::Active,
                reference: activated["reference"].as_str().unwrap().into(),
                evidence: activated,
            },
        },
    )
    .await?;
    finish(&hub,&run,RoundState::Completed,format!("Candidate {} passed gates, merged as {}, and was activated; use its feedback for the next round.",verified.candidate,merged.url)).await
}
pub(super) async fn reconcile(
    hub: &im_storage::Client,
    c: Arc<Config>,
    run: &EvolutionRun,
) -> Result<()> {
    let Some(round) = run.rounds.last() else {
        return Ok(());
    };
    if round.state == RoundState::Accepted
        && round
            .activation
            .as_ref()
            .is_some_and(|a| a.state == contracts::ActivationState::Active)
    {
        return finish(
            hub,
            run,
            RoundState::Completed,
            "Verified activation recovered; continue from the accepted version.".into(),
        )
        .await;
    }
    if !matches!(
        round.state,
        RoundState::Running | RoundState::Assessed | RoundState::Accepted
    ) {
        return Ok(());
    }
    let mut after = String::new();
    let goal = loop {
        let page = hub
            .goals(&run.input.group_id, "software-evolution", &after, false)
            .await?;
        let goals: Vec<Goal> = serde_json::from_value(page["goals"].clone())?;
        if let Some(goal) = goals.into_iter().find(|g| g.input.id == round.goal_id) {
            break goal;
        }
        match page["next_after"].as_str() {
            Some(next) if next > after.as_str() => after = next.into(),
            _ => anyhow::bail!("iteration goal unavailable"),
        }
    };
    if round
        .started_ms
        .saturating_add(run.input.max_seconds.saturating_mul(1000))
        <= crystal::now_ms()
        && goal.work.iter().all(|w| {
            w.state != WorkState::Running || w.deadline_ms.is_some_and(|d| d <= crystal::now_ms())
        })
        && goal
            .work
            .iter()
            .any(|w| matches!(w.state, WorkState::Ready | WorkState::Running))
    {
        return finish(
            hub,
            run,
            RoundState::Failed,
            "Round execution deadline expired; retain the baseline.".into(),
        )
        .await;
    }
    if goal.work.iter().any(|w| w.state == WorkState::Failed)
        && goal.work.iter().all(|w| {
            w.state != WorkState::Running || w.deadline_ms.is_some_and(|d| d <= crystal::now_ms())
        })
    {
        return finish(
            hub,
            run,
            RoundState::Failed,
            "An execution failed; retain baseline and use the recorded counterexample.".into(),
        )
        .await;
    }
    if goal
        .work
        .iter()
        .any(|w| matches!(w.state, WorkState::Ready | WorkState::Running))
    {
        return Ok(());
    }
    let key = round.goal_id.clone();
    let retry_path = c
        .state_dir
        .join("private/software-delivery")
        .join(&run.input.id)
        .join("retry.json");
    if retry_path.exists()
        && storage::read::<Value>(&retry_path)?["after_ms"]
            .as_u64()
            .is_some_and(|after| after > crystal::now_ms())
    {
        return Ok(());
    }
    {
        let mut running = RUNNING.get_or_init(Default::default).lock().unwrap();
        if !running.is_empty() {
            return Ok(());
        }
        running.insert(key.clone());
    }
    if let Err(error) = storage::write(&retry_path, &json!({"after_ms":crystal::now_ms()+60000})) {
        RUNNING.get().unwrap().lock().unwrap().remove(&key);
        return Err(error);
    }
    let hub = hub.clone();
    let run = run.clone();
    tokio::spawn(async move {
        let _guard = Running(key);
        if let Err(error) = deliver(hub.clone(), c.clone(), run.clone(), goal).await {
            let public_error = error.to_string().chars().take(1500).collect::<String>();
            let _ = hub.publish_as("operator",crystal::Publish {group_id:run.input.group_id.clone(),
                request_id:format!("delivery-wait-{}",&storage::digest(format!("{}:{public_error}",run.rounds.last().unwrap().goal_id).as_bytes())[..40]),
                text:"Software delivery is waiting for host verification, CI or activation. The host retained the diagnostic evidence.".into(),reply_to:None}).await;
            let _ = storage::write(
                &c.state_dir
                    .join("private/software-delivery")
                    .join(&run.input.id)
                    .join("last-error.json"),
                &json!({"observed_ms":crystal::now_ms(),"error":format!("{error:#}")}),
            );
            // Preserve accepted candidates for retryable CI/delivery checks. No activation is inferred.
            if let Ok(latest) = hub.evolution_run(&run.input.id).await
                && latest
                    .rounds
                    .last()
                    .is_some_and(|r| r.state != RoundState::Accepted)
            {
                let _ = finish(
                    &hub,
                    &run,
                    RoundState::Failed,
                    "Host verification blocked; retain the baseline and inspect the private diagnostic evidence.".into(),
                )
                .await;
            }
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn candidate_snapshot_budget_preserves_legacy_policy_and_full_gate_arguments() {
        let legacy = r#"{"schema_version":1,"repositories":{"example":{"repository":"owner/repo","tests":[["cargo","test"]],"activation":["host","activate"]}}}"#;
        let policy: DeliveryPolicy = serde_json::from_str(legacy).unwrap();
        let bytes = serde_json::to_vec(&policy).unwrap();
        assert_eq!(bytes, legacy.as_bytes());
        assert_eq!(
            storage::digest(&bytes),
            "760d52ac5569c44711a14013d1dff8aa3f2f4dd36d8242933f48b50471b39d8e"
        );
        let expected = [
            "qualitygate",
            "check",
            "--root",
            "/candidate",
            "--diff",
            "baseline..candidate",
            "--profile",
            "full",
            "--format",
            "json",
        ]
        .map(str::to_owned)
        .to_vec();
        assert_eq!(
            policy.repositories["example"].qualitygate_command(
                "qualitygate".into(),
                Path::new("/candidate"),
                "baseline",
                "candidate",
            ),
            expected
        );
        for budget in [1, 5, 8] {
            let mut value: Value = serde_json::from_str(legacy).unwrap();
            value["repositories"]["example"]["qualitygate_snapshot_max_file_mib"] = json!(budget);
            let configured: DeliveryPolicy = serde_json::from_value(value).unwrap();
            let args = configured.repositories["example"].qualitygate_command(
                "qualitygate".into(),
                Path::new("/candidate"),
                "baseline",
                "candidate",
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
                serde_json::to_value(&configured).unwrap()["repositories"]["example"]["qualitygate_snapshot_max_file_mib"],
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
            value["repositories"]["example"]["qualitygate_snapshot_max_file_mib"] = invalid;
            assert!(serde_json::from_value::<DeliveryPolicy>(value).is_err());
        }
    }
    #[test]
    fn activation_adapters_decode_native_cli_envelopes_and_raw_receipts() {
        let receipt = json!({"schema_version":1,"active":true,"goal_id":"goal",
            "candidate_commit":"a".repeat(40),"merged_commit":"b".repeat(40),
            "artifact_sha256":"c".repeat(64),"reference":"artifact/version", "activation_claim_id":"claim"});
        for encoded in [receipt.clone(), json!({"ok":true,"result":receipt})] {
            let decoded = activation_output(&serde_json::to_vec(&encoded).unwrap()).unwrap();
            assert_eq!(decoded, receipt);
            validate_activation(&decoded, &"a".repeat(40), &"b".repeat(40), "goal").unwrap();
        }
        for malformed in [
            json!({"ok":true,"error":"failed","result":receipt}),
            json!({"ok":true,"active":false,"result":receipt}),
            json!({"ok":false,"result":receipt}),
            json!({"ok":"true","result":receipt}),
            json!({"ok":true}),
            json!({"ok":true,"result":null}),
            json!({"schema_version":2,"active":true}),
        ] {
            assert!(activation_output(&serde_json::to_vec(&malformed).unwrap()).is_err());
        }
        let pending =
            json!({"schema_version":1,"active":false,"phase":contracts::ActivationPhase::Prepared});
        let decoded =
            activation_output(&serde_json::to_vec(&json!({"ok":true,"result":pending})).unwrap())
                .unwrap();
        assert_eq!(decoded, pending);
        assert!(validate_activation(&decoded, &"a".repeat(40), &"b".repeat(40), "goal").is_err());
    }
    #[test]
    fn activation_requires_actual_merged_source_and_current_goal() {
        let mut receipt = json!({"schema_version":1,"active":true,"candidate_commit":"a".repeat(40),"merged_commit":"b".repeat(40),"goal_id":"goal","artifact_sha256":"c".repeat(64),"reference":"artifact/version"});
        assert!(validate_activation(&receipt, &"a".repeat(40), &"b".repeat(40), "goal").is_ok());
        assert!(validate_activation(&receipt, &"a".repeat(40), &"d".repeat(40), "goal").is_err());
        assert!(
            validate_activation(&receipt, &"a".repeat(40), &"b".repeat(40), "another-goal")
                .is_err()
        );
        receipt["active"] = json!(false);
        assert!(validate_activation(&receipt, &"a".repeat(40), &"b".repeat(40), "goal").is_err());
        receipt["active"] = json!(true);
        receipt["artifact_sha256"] = json!("z".repeat(64));
        assert!(validate_activation(&receipt, &"a".repeat(40), &"b".repeat(40), "goal").is_err());
    }
    #[test]
    fn host_commands_fail_closed_on_nonzero_or_deadline() {
        let root = std::env::temp_dir();
        assert!(command(&["/bin/sh".into(), "-c".into(), "exit 7".into()], &root, 2).is_err());
        assert!(command(&["/bin/sh".into(), "-c".into(), "sleep 5".into()], &root, 1).is_err());
        assert_eq!(
            command(
                &["/bin/sh".into(), "-c".into(), "printf done".into()],
                &root,
                2
            )
            .unwrap(),
            b"done"
        );
    }
}
