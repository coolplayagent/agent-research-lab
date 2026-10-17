//! File-based host adapters for prompt evolution. Gate inputs and receipts are
//! kept outside agent workspaces; candidate reports never supply approval flags.
//! These path checks do not replace OS-level filesystem isolation.

use anyhow::{Context, Result, bail, ensure};
use delivery::VerificationArtifact;
use evolution::{
    AblationStudy, EvaluationContext, EvaluationDecision, ExplorationBudget, ExplorationOutcome,
    HoldoutIsolation, PromptDefinition, PromptRegistry, PromptVersion, QuarantinedRuleProposal,
    StrategyPortfolio, TrialObservation, Variant, evaluate_novel_capability, evaluate_promotion,
};
use freshness::{RemoteClient, RemoteSnapshot};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LineageRequest {
    pub registry_file: PathBuf,
    pub study: evolution::lineage::Study,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LineageReceipt {
    pub schema_version: u32,
    pub registry: PromptRegistry,
    pub study: evolution::lineage::Study,
    pub report: evolution::lineage::Report,
}
impl LineageReceipt {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.schema_version == 1, "unsupported lineage receipt");
        let recomputed = evolution::lineage::diagnose(&self.registry, &self.study)?;
        ensure!(
            serde_json::to_value(recomputed)? == serde_json::to_value(&self.report)?,
            "lineage receipt does not match its evidence"
        );
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PromptRegistration {
    pub definition: PromptDefinition,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PortfolioValidation {
    pub registry_file: PathBuf,
    pub portfolio: StrategyPortfolio,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PromotionRequest {
    /// Host-controlled expected versions, separate from observation reports.
    pub context_file: PathBuf,
    pub upstream: RemoteSnapshot,
    pub skills_manifest: VerificationArtifact,
    pub holdout_isolation: HoldoutIsolation,
    pub observations: Vec<TrialObservation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NoveltyRequest {
    pub context_file: PathBuf,
    pub upstream: RemoteSnapshot,
    pub skills_manifest: VerificationArtifact,
    pub holdout_isolation: HoldoutIsolation,
    pub observations: Vec<TrialObservation>,
    pub ablations: Vec<AblationStudy>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvaluationKind {
    Promotion,
    Novelty,
}

/// Full host receipt enables downstream consumers to recompute the gate instead
/// of trusting an `approved` field, including after a controller restart.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvaluationReceipt {
    pub schema_version: u32,
    /// Missing only on historical receipts, which remain readable but cannot
    /// authorize new promotion or exploration-budget increases.
    #[serde(default)]
    pub upstream: Option<RemoteSnapshot>,
    #[serde(default)]
    pub skills_manifest: Option<VerificationArtifact>,
    pub kind: EvaluationKind,
    pub context: EvaluationContext,
    pub observations: Vec<TrialObservation>,
    pub ablations: Vec<AblationStudy>,
    pub decision: EvaluationDecision,
}

impl EvaluationReceipt {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == 2,
            "unsupported evaluation receipt version"
        );
        let upstream = self.upstream.as_ref().context(
            "old evaluation receipt lacks an upstream baseline; re-evaluation is required",
        )?;
        validate_evaluation_baseline(&self.context, upstream)?;
        let skills = self.skills_manifest.as_ref().context(
            "old evaluation receipt lacks a hashed skills manifest; re-evaluation is required",
        )?;
        skills.validate_skills()?;
        let mut canonical = self.observations.clone();
        canonicalize_observations(&mut canonical);
        ensure!(
            serde_json::to_value(&canonical)? == serde_json::to_value(&self.observations)?,
            "evaluation observations are not in canonical order"
        );
        let mut ablations = self.ablations.clone();
        canonicalize_ablations(&mut ablations);
        ensure!(
            serde_json::to_value(&ablations)? == serde_json::to_value(&self.ablations)?,
            "ablation evidence is not in canonical order"
        );
        let mut recomputed = match self.kind {
            EvaluationKind::Promotion => {
                ensure!(
                    self.ablations.is_empty(),
                    "promotion receipt cannot contain ablations"
                );
                evaluate_promotion(&self.context, &self.observations)?
            }
            EvaluationKind::Novelty => {
                evaluate_novel_capability(&self.context, &self.observations, &self.ablations)?
            }
        };
        bind_upstream_digest(&mut recomputed, upstream, skills)?;
        ensure!(
            serde_json::to_value(&recomputed)? == serde_json::to_value(&self.decision)?,
            "evaluation receipt does not match recomputed evidence"
        );
        Ok(())
    }

    pub fn critical_regression(&self) -> bool {
        self.observations
            .iter()
            .any(|trial| trial.variant == Variant::Candidate && trial.critical_regression)
            || self
                .ablations
                .iter()
                .flat_map(|study| &study.observations)
                .any(|trial| trial.variant == Variant::Candidate && trial.critical_regression)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExperimentReceipt {
    /// Human-facing label. Durable deduplication uses the evidence digest, so
    /// relabeling the same experiment cannot increase the exploration share.
    pub experiment_id: String,
    pub evaluation_receipt: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExplorationUpdate {
    pub holdout_isolation: HoldoutIsolation,
    pub experiments: Vec<ExperimentReceipt>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleProposalRequest {
    pub parent_policy_version: String,
    pub proposed_rule: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StrategyPromotionRequest {
    pub holdout_isolation: HoldoutIsolation,
    pub context_file: PathBuf,
    pub evaluation_receipt: PathBuf,
    pub registry_file: PathBuf,
    pub candidate_version: String,
    pub role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectedStrategy {
    pub stable_version: String,
    pub task_kind: String,
    pub previous_stable_versions: Vec<String>,
    pub evidence_digest: String,
    pub policy_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StrategySelections {
    pub schema_version: u32,
    pub roles: BTreeMap<String, SelectedStrategy>,
}

impl Default for StrategySelections {
    fn default() -> Self {
        Self {
            schema_version: 1,
            roles: BTreeMap::new(),
        }
    }
}

impl StrategySelections {
    fn validate(&self, registry: &PromptRegistry) -> Result<()> {
        ensure!(
            self.schema_version == 1,
            "unsupported strategy selection version"
        );
        registry.validate()?;
        for (role, selection) in &self.roles {
            ensure!(
                !selection.evidence_digest.is_empty() && !selection.policy_version.is_empty(),
                "selected strategy is missing evidence binding"
            );
            ensure!(
                selection.previous_stable_versions.len() <= 5,
                "strategy history exceeds five versions"
            );
            let mut seen = BTreeSet::new();
            for version in std::iter::once(&selection.stable_version)
                .chain(&selection.previous_stable_versions)
            {
                ensure!(
                    seen.insert(version),
                    "duplicate selected or historical strategy"
                );
                let prompt = registry
                    .get(version)
                    .context("selected strategy is not registered")?;
                ensure!(
                    prompt.definition().role == *role
                        && prompt.definition().task_kind == selection.task_kind,
                    "selected strategy role or task kind mismatch"
                );
            }
        }
        Ok(())
    }
}

/// New tasks freeze the selected version at enqueue time. Missing selection is
/// an initial-template fallback; corrupt state is an error, never a fallback.
pub fn resolve_selection(state_dir: &Path, role: &str) -> Result<Option<String>> {
    let path = state_dir.join("evolution/selections.json");
    if !path.exists() {
        return Ok(None);
    }
    let selections: StrategySelections = read_strict(&path)?;
    let registry: PromptRegistry = read_strict(&state_dir.join("evolution/prompts.json"))?;
    selections.validate(&registry)?;
    Ok(selections
        .roles
        .get(role)
        .map(|selection| selection.stable_version.clone()))
}

/// Resolve an explicitly selected immutable strategy for a runtime task. This
/// does not choose or activate a candidate on the caller's behalf.
pub fn resolve_registered_prompt(
    registry_file: &Path,
    version: &str,
    role: &str,
) -> Result<PromptVersion> {
    let registry: PromptRegistry = read_strict(registry_file)?;
    registry.validate()?;
    let prompt = registry
        .get(version)
        .context("selected prompt version is not registered")?;
    ensure!(
        prompt.definition().role == role,
        "selected prompt role does not match task role"
    );
    Ok(prompt.clone())
}

/// Turn a structured agent proposal into an immutable candidate. Registration
/// preserves ancestry; choosing the stable portfolio still requires the host
/// promotion path. JSON contains PromptRegistration, never an approval field.
pub fn register_prompt_candidate(proposal_file: &Path, registry_file: &Path) -> Result<Value> {
    execute("prompt-register", proposal_file, Some(registry_file))
}

/// Paths in JSON are interpreted relative to the request file, not the shell's
/// working directory. Stateful operations require an explicit output path.
pub fn execute(action: &str, input: &Path, output: Option<&Path>) -> Result<Value> {
    execute_with_client(action, input, output, &RemoteClient::default())
}

fn execute_with_client(
    action: &str,
    input: &Path,
    output: Option<&Path>,
    remote: &RemoteClient,
) -> Result<Value> {
    let input = input
        .canonicalize()
        .with_context(|| format!("cannot read {}", input.display()))?;
    let base = input.parent().context("request file has no parent")?;
    let output = output.map(resolve_destination).transpose()?;
    match action {
        "lineage" => {
            let request: LineageRequest = read_strict(&input)?;
            let registry_path = relative_to(base, &request.registry_file).canonicalize()?;
            let registry: PromptRegistry = read_strict(&registry_path)?;
            let report = evolution::lineage::diagnose(&registry, &request.study)?;
            if let Some(destination) = &output {
                ensure!(
                    destination != &input && destination != &registry_path,
                    "lineage report cannot replace its inputs"
                );
            }
            let receipt = LineageReceipt {
                schema_version: 1,
                registry,
                study: request.study,
                report,
            };
            receipt.validate()?;
            write_optional(output.as_deref(), &receipt)?;
            Ok(serde_json::to_value(&receipt.report)?)
        }
        "prompt-register" => {
            let request: PromptRegistration = read_strict(&input)?;
            let destination =
                output.context("prompt-register requires an explicit --output registry path")?;
            ensure!(
                destination != input,
                "registry output cannot replace its request file"
            );
            let _lock = storage::lock(&destination.with_extension("evolution.lock"))?;
            let mut registry: PromptRegistry = if destination.exists() {
                read_strict(&destination)?
            } else {
                PromptRegistry::default()
            };
            registry.validate()?;
            let prompt = PromptVersion::new(request.definition)?;
            let already_registered = registry.get(prompt.version()).is_some();
            let version = registry.register(prompt)?;
            storage::write(&destination, &registry)?;
            Ok(
                json!({"version":version,"registry_file":destination,"already_registered":already_registered}),
            )
        }
        "prompt-portfolio" => {
            let request: PortfolioValidation = read_strict(&input)?;
            let registry_path = relative_to(base, &request.registry_file);
            let registry: PromptRegistry = read_strict(&registry_path)?;
            request.portfolio.validate(&registry)?;
            let result = json!({"valid":true,"activated":false,"portfolio":request.portfolio});
            write_optional(output.as_deref(), &result)?;
            Ok(result)
        }
        "evaluate" => {
            let mut request: PromotionRequest = read_strict(&input)?;
            resolve_isolation(base, &mut request.holdout_isolation);
            let context_path = relative_to(base, &request.context_file);
            check_gate_paths(
                &input,
                &context_path,
                output.as_deref(),
                &request.holdout_isolation,
            )?;
            let context: EvaluationContext = read_strict(&context_path)?;
            validate_evaluation_baseline(&context, &request.upstream)?;
            prepare_skills_artifact(
                base,
                &mut request.skills_manifest,
                &request.holdout_isolation,
            )?;
            canonicalize_observations(&mut request.observations);
            let mut decision = evaluate_promotion(&context, &request.observations)?;
            bind_upstream_digest(&mut decision, &request.upstream, &request.skills_manifest)?;
            let receipt = EvaluationReceipt {
                schema_version: 2,
                upstream: Some(request.upstream),
                skills_manifest: Some(request.skills_manifest),
                kind: EvaluationKind::Promotion,
                context,
                observations: request.observations,
                ablations: vec![],
                decision,
            };
            emit_receipt(receipt, output.as_deref())
        }
        "novelty" => {
            let mut request: NoveltyRequest = read_strict(&input)?;
            resolve_isolation(base, &mut request.holdout_isolation);
            let context_path = relative_to(base, &request.context_file);
            check_gate_paths(
                &input,
                &context_path,
                output.as_deref(),
                &request.holdout_isolation,
            )?;
            let context: EvaluationContext = read_strict(&context_path)?;
            validate_evaluation_baseline(&context, &request.upstream)?;
            prepare_skills_artifact(
                base,
                &mut request.skills_manifest,
                &request.holdout_isolation,
            )?;
            canonicalize_observations(&mut request.observations);
            canonicalize_ablations(&mut request.ablations);
            let mut decision =
                evaluate_novel_capability(&context, &request.observations, &request.ablations)?;
            bind_upstream_digest(&mut decision, &request.upstream, &request.skills_manifest)?;
            let receipt = EvaluationReceipt {
                schema_version: 2,
                upstream: Some(request.upstream),
                skills_manifest: Some(request.skills_manifest),
                kind: EvaluationKind::Novelty,
                context,
                observations: request.observations,
                ablations: request.ablations,
                decision,
            };
            emit_receipt(receipt, output.as_deref())
        }
        "exploration" => {
            let mut request: ExplorationUpdate = read_strict(&input)?;
            ensure!(
                !request.experiments.is_empty(),
                "exploration update needs at least one experiment receipt"
            );
            resolve_isolation(base, &mut request.holdout_isolation);
            request.holdout_isolation.validate()?;
            ensure_host_path(&input, &request.holdout_isolation)?;
            let destination =
                output.context("exploration requires an explicit --output budget path")?;
            ensure_host_path(&destination, &request.holdout_isolation)?;
            ensure!(
                destination != input,
                "budget output cannot replace its request file"
            );
            let _lock = storage::lock(&destination.with_extension("evolution.lock"))?;
            let mut budget: ExplorationBudget = if destination.exists() {
                read_strict(&destination)?
            } else {
                ExplorationBudget::default()
            };
            budget.validate()?;
            let mut adjustments = Vec::new();
            let mut labels = BTreeSet::new();
            let mut recorded = Vec::new();
            for experiment in request.experiments {
                ensure!(
                    !experiment.experiment_id.trim().is_empty(),
                    "experiment label must not be empty"
                );
                ensure!(
                    labels.insert(experiment.experiment_id.clone()),
                    "duplicate experiment label in update"
                );
                let receipt_path = relative_to(base, &experiment.evaluation_receipt);
                ensure_host_path(&receipt_path, &request.holdout_isolation)?;
                ensure!(
                    resolve_destination(&receipt_path)? != destination,
                    "receipt cannot be the budget file"
                );
                let receipt: EvaluationReceipt = read_strict(&receipt_path)?;
                receipt.validate()?;
                remote.verify_remote_binding(
                    receipt
                        .upstream
                        .as_ref()
                        .context("missing evaluation upstream")?,
                )?;
                let skills = receipt
                    .skills_manifest
                    .as_ref()
                    .context("missing evaluation skills manifest")?;
                ensure_host_path(&skills.path, &request.holdout_isolation)?;
                skills.verify_skills(remote)?;
                let evidence_id = receipt.decision.evidence_digest.clone();
                if let Some(share) = budget.record(ExplorationOutcome {
                    experiment_id: evidence_id.clone(),
                    promoted_on_holdout: receipt.kind == EvaluationKind::Promotion
                        && receipt.decision.approved,
                    critical_regression: receipt.critical_regression(),
                })? {
                    adjustments.push(share);
                }
                recorded.push(
                    json!({"experiment_id":experiment.experiment_id,"evidence_digest":evidence_id}),
                );
            }
            // Write only after every update validates; a bad batch cannot
            // partially change the durable exploration budget.
            storage::write(&destination, &budget)?;
            Ok(
                json!({"budget_file":destination,"share_percent":budget.share_percent(),"pending_count":budget.pending_count(),"completed_window_shares":adjustments,"recorded":recorded}),
            )
        }
        "rule-proposal" => {
            let request: RuleProposalRequest = read_strict(&input)?;
            let proposal = QuarantinedRuleProposal::new(
                request.parent_policy_version,
                request.proposed_rule,
                request.reason,
            )?;
            let result = json!({"quarantined":true,"adopted":false,"proposal":proposal});
            write_optional(output.as_deref(), &result)?;
            Ok(result)
        }
        "promote" => {
            let mut request: StrategyPromotionRequest = read_strict(&input)?;
            resolve_isolation(base, &mut request.holdout_isolation);
            let context_path = relative_to(base, &request.context_file);
            let registry_path = relative_to(base, &request.registry_file);
            let receipt_path = relative_to(base, &request.evaluation_receipt);
            let destination =
                output.context("promote requires an explicit --output selections path")?;
            check_gate_paths(
                &input,
                &context_path,
                Some(&destination),
                &request.holdout_isolation,
            )?;
            for source in [&registry_path, &receipt_path] {
                ensure_host_path(source, &request.holdout_isolation)?;
                ensure!(
                    resolve_destination(source)? != destination,
                    "selection output cannot replace registry or receipt"
                );
            }
            let context: EvaluationContext = read_strict(&context_path)?;
            let receipt: EvaluationReceipt = read_strict(&receipt_path)?;
            receipt.validate()?;
            ensure!(
                receipt.kind == EvaluationKind::Promotion && receipt.decision.approved,
                "candidate has not passed the prompt promotion gate"
            );
            ensure!(
                serde_json::to_value(&context)? == serde_json::to_value(&receipt.context)?,
                "promotion receipt is stale for the expected host context"
            );
            ensure!(
                context.candidate.prompt_version == request.candidate_version,
                "receipt approves a different prompt version"
            );
            ensure!(
                context.baseline.source_commit == context.candidate.source_commit
                    && context.baseline.configuration_digest
                        == context.candidate.configuration_digest,
                "prompt activation requires unchanged source and model/tool/memory configuration"
            );
            let registry: PromptRegistry = read_strict(&registry_path)?;
            registry.validate()?;
            let candidate = registry
                .get(&request.candidate_version)
                .context("candidate prompt is not registered")?;
            let baseline = registry
                .get(&context.baseline.prompt_version)
                .context("baseline prompt is not registered")?;
            ensure!(
                candidate.definition().role == request.role
                    && baseline.definition().role == request.role,
                "promotion role does not match registered prompt roles"
            );
            ensure!(
                candidate.definition().task_kind == baseline.definition().task_kind,
                "promotion changes task kind"
            );
            ensure!(
                descends_from(
                    &registry,
                    &request.candidate_version,
                    &context.baseline.prompt_version
                ),
                "candidate does not descend from the evaluated baseline"
            );
            let _lock = storage::lock(&destination.with_extension("evolution.lock"))?;
            let mut selections: StrategySelections = if destination.exists() {
                read_strict(&destination)?
            } else {
                StrategySelections::default()
            };
            selections.validate(&registry)?;
            let upstream = receipt
                .upstream
                .as_ref()
                .context("missing promotion upstream baseline")?;
            // The paired run stays frozen, but its historical success cannot
            // authorize selection after the remote default branch advances.
            remote.verify_remote_binding(upstream)?;
            let skills = receipt
                .skills_manifest
                .as_ref()
                .context("missing promotion skills manifest")?;
            ensure_host_path(&skills.path, &request.holdout_isolation)?;
            skills.verify_skills(remote)?;
            if let Some(current) = selections.roles.get(&request.role) {
                if current.stable_version == request.candidate_version {
                    ensure!(
                        current.evidence_digest == receipt.decision.evidence_digest
                            && current.policy_version == context.candidate.policy_version,
                        "selected candidate is bound to a different promotion receipt"
                    );
                    return Ok(
                        json!({"selected":true,"already_selected":true,"role":request.role,"version":request.candidate_version,"selections_file":destination}),
                    );
                }
                ensure!(
                    current.stable_version == context.baseline.prompt_version,
                    "stale promotion: evaluated baseline is no longer the selected strategy"
                );
                ensure!(
                    current.policy_version == context.candidate.policy_version,
                    "formal policy changed; automatic selection cannot adopt a new policy"
                );
            }
            let mut history = selections
                .roles
                .get(&request.role)
                .map(|current| current.previous_stable_versions.clone())
                .unwrap_or_default();
            history.retain(|version| {
                version != &request.candidate_version && version != &context.baseline.prompt_version
            });
            history.insert(0, context.baseline.prompt_version.clone());
            history.truncate(5);
            selections.roles.insert(
                request.role.clone(),
                SelectedStrategy {
                    stable_version: request.candidate_version.clone(),
                    task_kind: candidate.definition().task_kind.clone(),
                    previous_stable_versions: history,
                    evidence_digest: receipt.decision.evidence_digest,
                    policy_version: context.candidate.policy_version,
                },
            );
            selections.validate(&registry)?;
            skills.verify_skills(remote)?;
            remote.verify_remote_binding(upstream)?;
            storage::write(&destination, &selections)?;
            Ok(
                json!({"selected":true,"already_selected":false,"role":request.role,"version":request.candidate_version,"selections_file":destination}),
            )
        }
        "holdout-validate" => {
            let mut isolation: HoldoutIsolation = read_strict(&input)?;
            resolve_isolation(base, &mut isolation);
            isolation.validate()?;
            let result = json!({"paths_valid":true,"os_isolation_verified":false,"holdout_isolation":isolation});
            write_optional(output.as_deref(), &result)?;
            Ok(result)
        }
        _ => bail!("unknown evolution action: {action}"),
    }
}

fn validate_evaluation_baseline(
    context: &EvaluationContext,
    upstream: &RemoteSnapshot,
) -> Result<()> {
    upstream.validate()?;
    ensure!(
        context.baseline.source_commit == upstream.commit,
        "evaluation source differs from the captured upstream default baseline"
    );
    Ok(())
}

fn bind_upstream_digest(
    decision: &mut EvaluationDecision,
    upstream: &RemoteSnapshot,
    skills: &VerificationArtifact,
) -> Result<()> {
    decision.evidence_digest = storage::digest(&serde_json::to_vec(&(
        &decision.evidence_digest,
        &upstream.repository,
        &upstream.default_branch,
        &upstream.commit,
        &skills.sha256,
    ))?);
    Ok(())
}

fn prepare_skills_artifact(
    base: &Path,
    artifact: &mut VerificationArtifact,
    isolation: &HoldoutIsolation,
) -> Result<()> {
    artifact.path = relative_to(base, &artifact.path).canonicalize()?;
    ensure_host_path(&artifact.path, isolation)?;
    artifact.validate_skills()
}

fn descends_from(registry: &PromptRegistry, candidate: &str, baseline: &str) -> bool {
    let mut pending = vec![candidate];
    let mut seen = BTreeSet::new();
    while let Some(version) = pending.pop() {
        if !seen.insert(version) {
            continue;
        }
        if let Some(prompt) = registry.get(version) {
            for parent in &prompt.definition().parent_versions {
                if parent == baseline {
                    return true;
                }
                pending.push(parent);
            }
        }
    }
    false
}

fn emit_receipt(receipt: EvaluationReceipt, output: Option<&Path>) -> Result<Value> {
    receipt.validate()?;
    write_optional(output, &receipt)?;
    Ok(json!({"kind":receipt.kind,"decision":receipt.decision,"receipt_file":output}))
}

fn canonicalize_observations(observations: &mut [TrialObservation]) {
    observations.sort_by(|a, b| {
        (&a.task_family, &a.trial_id, a.variant == Variant::Candidate).cmp(&(
            &b.task_family,
            &b.trial_id,
            b.variant == Variant::Candidate,
        ))
    });
}

fn canonicalize_ablations(ablations: &mut [AblationStudy]) {
    ablations.sort_by(|a, b| a.removed_component.cmp(&b.removed_component));
    for ablation in ablations {
        canonicalize_observations(&mut ablation.observations);
    }
}

fn check_gate_paths(
    input: &Path,
    context: &Path,
    output: Option<&Path>,
    isolation: &HoldoutIsolation,
) -> Result<()> {
    isolation.validate()?;
    ensure_host_path(input, isolation)?;
    ensure_host_path(context, isolation)?;
    if let Some(output) = output {
        ensure_host_path(output, isolation)?;
        ensure!(
            output != input && output != resolve_destination(context)?,
            "receipt cannot replace its request or expected context"
        );
    }
    Ok(())
}

fn ensure_host_path(path: &Path, isolation: &HoldoutIsolation) -> Result<()> {
    let resolved = resolve_destination(path)?;
    for exposed in isolation
        .optimizer_roots
        .iter()
        .chain(&isolation.shared_knowledge_roots)
        .chain(&isolation.shared_memory_roots)
    {
        let exposed = exposed.canonicalize()?;
        ensure!(
            !resolved.starts_with(exposed),
            "gate requests, expected context and receipts must remain outside agent workspaces and shared stores"
        );
    }
    Ok(())
}

fn relative_to(base: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    }
}

fn resolve_isolation(base: &Path, isolation: &mut HoldoutIsolation) {
    isolation.private_root = relative_to(base, &isolation.private_root);
    for path in isolation
        .optimizer_roots
        .iter_mut()
        .chain(&mut isolation.shared_knowledge_roots)
        .chain(&mut isolation.shared_memory_roots)
    {
        *path = relative_to(base, path);
    }
}

fn resolve_destination(path: &Path) -> Result<PathBuf> {
    if path.exists() {
        ensure!(
            !path.symlink_metadata()?.file_type().is_symlink(),
            "output or host evidence path cannot be a symlink"
        );
        return Ok(path.canonicalize()?);
    }
    let filename = path.file_name().context("path needs a filename")?;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    Ok(parent
        .canonicalize()
        .with_context(|| format!("parent directory must exist: {}", parent.display()))?
        .join(filename))
}

fn read_strict<T: DeserializeOwned + Serialize>(path: &Path) -> Result<T> {
    let value: Value = serde_json::from_slice(
        &fs::read(path).with_context(|| format!("cannot read {}", path.display()))?,
    )?;
    let decoded: T = serde_json::from_value(value.clone())
        .with_context(|| format!("invalid request or evidence: {}", path.display()))?;
    reject_extra_fields(&value, &serde_json::to_value(&decoded)?, "$")?;
    Ok(decoded)
}

// The domain structs intentionally support persistence. At the CLI boundary,
// also reject unknown nested keys so injected approval fields cannot disappear
// silently during serde deserialization.
fn reject_extra_fields(input: &Value, typed: &Value, location: &str) -> Result<()> {
    match (input, typed) {
        (Value::Object(input), Value::Object(typed)) => {
            for (key, value) in input {
                let next = format!("{location}.{key}");
                let expected = typed
                    .get(key)
                    .with_context(|| format!("unknown field {next}"))?;
                reject_extra_fields(value, expected, &next)?;
            }
        }
        (Value::Array(input), Value::Array(typed)) => {
            for (index, (input, typed)) in input.iter().zip(typed).enumerate() {
                reject_extra_fields(input, typed, &format!("{location}[{index}]"))?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn write_optional(path: Option<&Path>, value: &impl Serialize) -> Result<()> {
    if let Some(path) = path {
        storage::write(path, value)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lineage_receipt_recomputes_evidence_and_preserves_registry_aliases() {
        let temp = setup();
        let input = temp.path().join("host/register.json");
        let registry = temp.path().join("host/registry.json");
        storage::write(&input, &example("prompt-register")).unwrap();
        execute("prompt-register", &input, Some(&registry)).unwrap();
        let request = temp.path().join("host/lineage.json");
        let study = evolution::lineage::Study {
            task_kind: "long_horizon".into(),
            policy_version: "a".repeat(40),
            superpod_commit: "b".repeat(40),
            expected: BTreeMap::new(),
            observations: vec![],
            pending_evaluations: BTreeMap::new(),
            pending_expansions: BTreeMap::new(),
            evaluation_budget: 10,
        };
        let output = temp.path().join("host/report.json");
        storage::write(
            &request,
            &LineageRequest {
                registry_file: "registry.json".into(),
                study,
            },
        )
        .unwrap();
        execute("lineage", &request, Some(&output)).unwrap();
        let mut receipt: LineageReceipt = storage::read(&output).unwrap();
        receipt.validate().unwrap();
        receipt.report.budget_remaining = 0;
        assert!(receipt.validate().is_err());
        let alias = temp.path().join("host/alias.json");
        std::os::unix::fs::symlink(&registry, &alias).unwrap();
        assert!(execute("lineage", &request, Some(&alias)).is_err());
        read_strict::<PromptRegistry>(&registry)
            .unwrap()
            .validate()
            .unwrap();
    }

    fn mock_remote(root: &Path) -> RemoteClient {
        use std::os::unix::fs::PermissionsExt;
        let head = root.join("current-upstream.json");
        if !head.exists() {
            storage::write(&head, &json!({"sha":"a".repeat(40)})).unwrap();
        }
        let gh = root.join("fake-gh");
        let quoted = head.to_string_lossy().replace('\'', "'\\''");
        let release = root.join("current-release.json");
        if !release.exists() {
            storage::write(
                &release,
                &json!({"id":1,"tag_name":"v1","draft":false,"prerelease":false}),
            )
            .unwrap();
        }
        let release = release.to_string_lossy().replace('\'', "'\\''");
        fs::write(&gh, format!("#!/bin/sh\nset -eu\ncase \"$1\" in\nrepo) printf '%s\\n' '{{\"nameWithOwner\":\"coolplayagent/example\",\"defaultBranchRef\":{{\"name\":\"main\"}}}}';;\napi) case \"$2\" in */releases/latest) cat '{release}';; *) cat '{quoted}';; esac;;\n*) exit 92;;\nesac\n")).unwrap();
        fs::set_permissions(&gh, fs::Permissions::from_mode(0o755)).unwrap();
        RemoteClient {
            git: PathBuf::from("git"),
            gh,
            timeout: std::time::Duration::from_secs(2),
        }
    }

    fn execute_fixture(action: &str, input: &Path, output: Option<&Path>) -> Result<Value> {
        let host = input.parent().unwrap();
        execute_with_client(action, input, output, &mock_remote(host))
    }

    fn example(name: &str) -> Value {
        serde_json::from_str(match name {
            "prompt-register" => include_str!("../../../examples/evolution/prompt-register.json"),
            "context" => include_str!("../../../examples/evolution/context.json"),
            "evaluate" => include_str!("../../../examples/evolution/evaluate.json"),
            "novelty" => include_str!("../../../examples/evolution/novelty.json"),
            "exploration" => include_str!("../../../examples/evolution/exploration.json"),
            "rule-proposal" => include_str!("../../../examples/evolution/rule-proposal.json"),
            "promote" => include_str!("../../../examples/evolution/promote.json"),
            _ => panic!("unknown fixture"),
        })
        .unwrap()
    }

    fn setup() -> tempfile::TempDir {
        let temp = tempfile::tempdir().unwrap();
        for name in ["host", "holdout", "optimizer", "superpod", "memory"] {
            fs::create_dir(temp.path().join(name)).unwrap();
        }
        temp
    }

    fn prepare_gate(temp: &Path, action: &str) -> PathBuf {
        let mut request = example(action);
        request["context_file"] = "context.json".into();
        request["holdout_isolation"] = json!({
            "private_root":"../holdout","optimizer_roots":["../optimizer"],
            "shared_knowledge_roots":["../superpod"],"shared_memory_roots":["../memory"]
        });
        let installed = temp.join("host/verified-skill");
        fs::create_dir_all(&installed).unwrap();
        fs::write(
            installed.join("SKILL.md"),
            "---\nname: verified-skill\n---\nFixture",
        )
        .unwrap();
        fs::write(installed.join("cli"), "verified runtime").unwrap();
        let manifest = temp.join("host/skills-manifest.json");
        storage::write(&manifest, &json!({"schema_version":1,"skills":[{"name":"verified-skill","repository":"coolplayagent/example","installed_path":installed,"runtime_path":installed.join("cli"),"tree_sha256":freshness::skill_tree_digest(&installed).unwrap(),"runtime_sha256":delivery::file_sha256(&installed.join("cli")).unwrap(),"release_tag":"v1","release_id":1,"source_commit":null,"checked_at":"2026-10-09T00:00:00Z"}]})).unwrap();
        request["skills_manifest"] = json!({"path":"skills-manifest.json","sha256":delivery::file_sha256(&manifest).unwrap()});
        storage::write(&temp.join("host/context.json"), &example("context")).unwrap();
        let path = temp.join(format!("host/{action}.json"));
        storage::write(&path, &request).unwrap();
        path
    }

    #[test]
    fn example_prompt_registration_is_idempotent_and_tampering_fails() {
        let temp = setup();
        let input = temp.path().join("host/request.json");
        let output = temp.path().join("host/registry.json");
        storage::write(&input, &example("prompt-register")).unwrap();
        assert!(execute("prompt-register", &input, None).is_err());
        let result = execute("prompt-register", &input, Some(&output)).unwrap();
        assert_eq!(result["already_registered"], false);
        assert_eq!(
            execute("prompt-register", &input, Some(&output)).unwrap()["already_registered"],
            true
        );
        let registry: PromptRegistry = read_strict(&output).unwrap();
        registry.validate().unwrap();
        let version = result["version"].as_str().unwrap();
        let mut changed: Value = storage::read(&output).unwrap();
        changed["versions"][version]["definition"]["content"] = "tampered".into();
        storage::write(&output, &changed).unwrap();
        assert!(execute("prompt-register", &input, Some(&output)).is_err());
    }

    #[test]
    fn examples_evaluate_and_novelty_produce_recomputable_receipts() {
        let temp = setup();
        for action in ["evaluate", "novelty"] {
            let input = prepare_gate(temp.path(), action);
            let output = temp.path().join(format!("host/{action}-receipt.json"));
            let result = execute(action, &input, Some(&output)).unwrap();
            assert_eq!(result["decision"]["approved"], true);
            let receipt: EvaluationReceipt = read_strict(&output).unwrap();
            receipt.validate().unwrap();
            let mut tampered = receipt;
            tampered.decision.approved = false;
            assert!(tampered.validate().is_err());
        }
    }

    #[test]
    fn candidates_cannot_supply_approval_or_host_gate_files() {
        let temp = setup();
        let input = prepare_gate(temp.path(), "evaluate");
        let mut request: Value = storage::read(&input).unwrap();
        request["observations"][0]["approved"] = true.into();
        storage::write(&input, &request).unwrap();
        assert!(execute("evaluate", &input, None).is_err());
        let input = prepare_gate(temp.path(), "evaluate");
        let agent_output = temp.path().join("optimizer/receipt.json");
        assert!(execute("evaluate", &input, Some(&agent_output)).is_err());
        let mut request: Value = storage::read(&input).unwrap();
        storage::write(
            &temp.path().join("optimizer/context.json"),
            &example("context"),
        )
        .unwrap();
        request["context_file"] = "../optimizer/context.json".into();
        storage::write(&input, &request).unwrap();
        assert!(execute("evaluate", &input, None).is_err());
    }

    #[test]
    fn exploration_persists_raw_budget_and_rejects_replay_without_partial_writes() {
        let temp = setup();
        let gate = prepare_gate(temp.path(), "evaluate");
        let receipt = temp.path().join("host/evaluation-receipt.json");
        execute("evaluate", &gate, Some(&receipt)).unwrap();
        let mut request = example("exploration");
        request["holdout_isolation"] = serde_json::to_value(
            read_strict::<PromotionRequest>(&gate)
                .unwrap()
                .holdout_isolation,
        )
        .unwrap();
        request["experiments"] =
            json!([{"experiment_id":"first","evaluation_receipt":"evaluation-receipt.json"}]);
        let input = temp.path().join("host/exploration.json");
        let output = temp.path().join("host/budget.json");
        storage::write(&input, &request).unwrap();
        execute_fixture("exploration", &input, Some(&output)).unwrap();
        let budget: ExplorationBudget = read_strict(&output).unwrap();
        assert_eq!(budget.pending_count(), 1);
        assert_eq!(budget.share_percent(), 25);
        let before = fs::read(&output).unwrap();
        request["experiments"] = json!([
            {"experiment_id":"second","evaluation_receipt":"evaluation-receipt.json"},
            {"experiment_id":"first","evaluation_receipt":"evaluation-receipt.json"}
        ]);
        storage::write(&input, &request).unwrap();
        assert!(execute_fixture("exploration", &input, Some(&output)).is_err());
        assert_eq!(before, fs::read(&output).unwrap());
    }

    #[test]
    fn rule_example_stays_quarantined() {
        let temp = setup();
        let input = temp.path().join("host/rule.json");
        storage::write(&input, &example("rule-proposal")).unwrap();
        let result = execute("rule-proposal", &input, None).unwrap();
        assert_eq!(result["quarantined"], true);
        assert_eq!(result["adopted"], false);
    }

    #[test]
    fn promotion_activates_only_bound_candidate_and_is_idempotent() {
        let temp = setup();
        fs::create_dir(temp.path().join("evolution")).unwrap();
        assert_eq!(resolve_selection(temp.path(), "research").unwrap(), None);
        let mut registry = PromptRegistry::default();
        let base_definition: PromptRegistration =
            serde_json::from_value(example("prompt-register")).unwrap();
        let baseline = registry
            .register(PromptVersion::new(base_definition.definition.clone()).unwrap())
            .unwrap();
        let mut definition = base_definition.definition.clone();
        definition.parent_versions = vec![baseline.clone()];
        definition
            .content
            .push_str(" Always run a falsification experiment.");
        let candidate = registry
            .register(PromptVersion::new(definition).unwrap())
            .unwrap();
        let registry_path = temp.path().join("evolution/prompts.json");
        storage::write(&registry_path, &registry).unwrap();
        let gate_input = prepare_gate(temp.path(), "evaluate");
        let mut gate: PromotionRequest = read_strict(&gate_input).unwrap();
        let context_path = temp.path().join("host/context.json");
        let mut context: EvaluationContext = read_strict(&context_path).unwrap();
        context.baseline.prompt_version = baseline.clone();
        context.candidate.prompt_version = candidate.clone();
        for trial in &mut gate.observations {
            trial.snapshot = if trial.variant == Variant::Baseline {
                context.baseline.clone()
            } else {
                context.candidate.clone()
            };
        }
        storage::write(&context_path, &context).unwrap();
        storage::write(&gate_input, &gate).unwrap();
        let receipt_path = temp.path().join("host/evaluation-receipt.json");
        execute("evaluate", &gate_input, Some(&receipt_path)).unwrap();
        let input = temp.path().join("host/promote.json");
        let output = temp.path().join("evolution/selections.json");
        let mut request = example("promote");
        request["candidate_version"] = candidate.clone().into();
        storage::write(&input, &request).unwrap();
        // Unchanged host context + a passing receipt are both stale once the
        // remote default advances; they must not create a selection.
        storage::write(
            &temp.path().join("host/current-upstream.json"),
            &json!({"sha":"e".repeat(40)}),
        )
        .unwrap();
        assert!(execute_fixture("promote", &input, Some(&output)).is_err());
        assert!(!output.exists());
        storage::write(
            &temp.path().join("host/current-upstream.json"),
            &json!({"sha":"a".repeat(40)}),
        )
        .unwrap();
        storage::write(
            &temp.path().join("host/current-release.json"),
            &json!({"id":2,"tag_name":"v2","draft":false,"prerelease":false}),
        )
        .unwrap();
        assert!(execute_fixture("promote", &input, Some(&output)).is_err());
        assert!(!output.exists());
        storage::write(
            &temp.path().join("host/current-release.json"),
            &json!({"id":1,"tag_name":"v1","draft":false,"prerelease":false}),
        )
        .unwrap();
        let result = execute_fixture("promote", &input, Some(&output)).unwrap();
        assert_eq!(result["already_selected"], false);
        assert_eq!(
            resolve_selection(temp.path(), "research").unwrap(),
            Some(candidate.clone())
        );
        let before = fs::read(&output).unwrap();
        assert_eq!(
            execute_fixture("promote", &input, Some(&output)).unwrap()["already_selected"],
            true
        );
        assert_eq!(before, fs::read(&output).unwrap());

        // A valid passing receipt for a new candidate against the previous
        // baseline cannot displace the strategy that has since been selected.
        let mut third = base_definition.definition;
        third.parent_versions = vec![baseline];
        third.content.push_str(" Another independent candidate.");
        let third = registry
            .register(PromptVersion::new(third).unwrap())
            .unwrap();
        storage::write(&registry_path, &registry).unwrap();
        context.candidate.prompt_version = third.clone();
        for trial in &mut gate.observations {
            if trial.variant == Variant::Candidate {
                trial.snapshot = context.candidate.clone();
            }
        }
        storage::write(&context_path, &context).unwrap();
        storage::write(&gate_input, &gate).unwrap();
        execute("evaluate", &gate_input, Some(&receipt_path)).unwrap();
        request["candidate_version"] = third.into();
        storage::write(&input, &request).unwrap();
        assert!(execute_fixture("promote", &input, Some(&output)).is_err());
        assert_eq!(before, fs::read(&output).unwrap());
        assert_eq!(
            resolve_selection(temp.path(), "research").unwrap(),
            Some(candidate)
        );
    }

    #[test]
    fn promotion_rejects_tampered_approval_and_different_registered_candidate() {
        let temp = setup();
        let gate_input = prepare_gate(temp.path(), "evaluate");
        let receipt_path = temp.path().join("host/evaluation-receipt.json");
        execute("evaluate", &gate_input, Some(&receipt_path)).unwrap();
        fs::create_dir(temp.path().join("evolution")).unwrap();
        storage::write(
            &temp.path().join("evolution/prompts.json"),
            &PromptRegistry::default(),
        )
        .unwrap();
        let input = temp.path().join("host/promote.json");
        storage::write(&input, &example("promote")).unwrap();
        let output = temp.path().join("evolution/selections.json");
        assert!(execute_fixture("promote", &input, Some(&output)).is_err());
        let mut receipt: Value = storage::read(&receipt_path).unwrap();
        receipt["decision"]["approved"] = false.into();
        storage::write(&receipt_path, &receipt).unwrap();
        assert!(execute_fixture("promote", &input, Some(&output)).is_err());
        assert!(!output.exists());
    }

    #[test]
    fn old_unbound_receipts_stay_readable_but_cannot_authorize_new_gates() {
        let temp = setup();
        let input = prepare_gate(temp.path(), "evaluate");
        let output = temp.path().join("host/receipt.json");
        execute("evaluate", &input, Some(&output)).unwrap();
        let mut old: Value = storage::read(&output).unwrap();
        old["schema_version"] = 1.into();
        old.as_object_mut().unwrap().remove("upstream");
        storage::write(&output, &old).unwrap();
        let old: EvaluationReceipt = read_strict(&output).unwrap();
        assert!(old.upstream.is_none());
        assert!(old.validate().is_err());
    }
}
