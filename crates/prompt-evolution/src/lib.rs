//! Evidence-bound prompt evolution. Decisions are advisory: callers still enforce
//! publication, installation and repository gates. Holdout path validation must
//! be paired with sandbox filesystem permissions when starting an agent.

pub mod lineage;

use anyhow::{Context, Result, bail, ensure};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

fn digest(value: &impl Serialize) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(value)?)))
}

fn nonempty(value: &str, field: &str) -> Result<()> {
    ensure!(!value.trim().is_empty(), "{field} must not be empty");
    Ok(())
}

/// The immutable payload of a prompt. Changing any field creates a new version.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PromptDefinition {
    pub parent_versions: Vec<String>,
    pub role: String,
    pub task_kind: String,
    pub content: String,
    pub change_reason: String,
    pub failure_conditions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PromptVersion {
    version: String,
    definition: PromptDefinition,
    created_at: DateTime<Utc>,
}

impl PromptVersion {
    pub fn new(mut definition: PromptDefinition) -> Result<Self> {
        nonempty(&definition.role, "role")?;
        nonempty(&definition.task_kind, "task kind")?;
        nonempty(&definition.content, "prompt content")?;
        nonempty(&definition.change_reason, "change reason")?;
        definition.parent_versions.sort();
        ensure!(
            definition.parent_versions.windows(2).all(|p| p[0] != p[1]),
            "duplicate prompt parent"
        );
        ensure!(
            !definition.failure_conditions.is_empty(),
            "at least one failure condition is required"
        );
        for condition in &definition.failure_conditions {
            nonempty(condition, "failure condition")?;
        }
        Ok(Self {
            version: digest(&definition)?,
            definition,
            created_at: Utc::now(),
        })
    }

    pub fn version(&self) -> &str {
        &self.version
    }

    pub fn definition(&self) -> &PromptDefinition {
        &self.definition
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    pub fn validate(&self) -> Result<()> {
        let rebuilt = Self::new(self.definition.clone())?;
        ensure!(
            self.version == rebuilt.version && self.definition == rebuilt.definition,
            "prompt version does not match its immutable payload"
        );
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PromptRegistry {
    versions: BTreeMap<String, PromptVersion>,
}

impl PromptRegistry {
    /// An identical version is idempotent; existing content is never overwritten.
    pub fn register(&mut self, prompt: PromptVersion) -> Result<String> {
        self.validate()?;
        prompt.validate()?;
        for parent in &prompt.definition.parent_versions {
            let ancestor = self.versions.get(parent).context("unknown prompt parent")?;
            ensure!(
                ancestor.definition.task_kind == prompt.definition.task_kind,
                "prompt parent belongs to a different task kind"
            );
        }
        let version = prompt.version.clone();
        if let Some(existing) = self.versions.get(&version) {
            ensure!(
                existing.definition == prompt.definition,
                "prompt hash collision"
            );
        } else {
            self.versions.insert(version.clone(), prompt);
        }
        Ok(version)
    }

    pub fn get(&self, version: &str) -> Option<&PromptVersion> {
        self.versions.get(version)
    }

    pub fn validate(&self) -> Result<()> {
        for (key, prompt) in &self.versions {
            prompt.validate()?;
            ensure!(key == &prompt.version, "prompt registry key mismatch");
            for parent in &prompt.definition.parent_versions {
                let ancestor = self
                    .versions
                    .get(parent)
                    .context("missing prompt ancestor")?;
                ensure!(
                    ancestor.definition.task_kind == prompt.definition.task_kind,
                    "prompt ancestry changed task kind"
                );
                ensure!(parent != key, "prompt cannot be its own parent");
            }
        }
        // Content addressing makes a valid cycle infeasible, but explicitly
        // check the graph as well when loading untrusted serialized registries.
        let mut completed = BTreeSet::new();
        let mut visiting = BTreeSet::new();
        for version in self.versions.keys() {
            self.visit(version, &mut visiting, &mut completed)?;
        }
        Ok(())
    }

    fn visit<'a>(
        &'a self,
        version: &'a str,
        visiting: &mut BTreeSet<&'a str>,
        completed: &mut BTreeSet<&'a str>,
    ) -> Result<()> {
        if completed.contains(version) {
            return Ok(());
        }
        ensure!(visiting.insert(version), "cyclic prompt ancestry");
        for parent in &self.versions[version].definition.parent_versions {
            self.visit(parent, visiting, completed)?;
        }
        visiting.remove(version);
        completed.insert(version);
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyPortfolio {
    pub task_kind: String,
    pub stable: String,
    pub specialized: Vec<String>,
    pub exploratory: Vec<String>,
}

impl StrategyPortfolio {
    pub fn validate(&self, registry: &PromptRegistry) -> Result<()> {
        registry.validate()?;
        nonempty(&self.task_kind, "task kind")?;
        ensure!(
            self.specialized.len() <= 2,
            "at most two specialized strategies"
        );
        ensure!(
            self.exploratory.len() <= 2,
            "at most two exploratory strategies"
        );
        let mut seen = BTreeSet::new();
        for version in std::iter::once(&self.stable)
            .chain(self.specialized.iter())
            .chain(self.exploratory.iter())
        {
            ensure!(
                seen.insert(version),
                "a strategy occupies multiple portfolio slots"
            );
            let prompt = registry.get(version).context("unknown portfolio prompt")?;
            ensure!(
                prompt.definition.task_kind == self.task_kind,
                "portfolio prompt belongs to a different task kind"
            );
        }
        Ok(())
    }
}

/// Every artifact able to affect an experiment must be represented by one of
/// these immutable identifiers. configuration_digest includes tool versions,
/// model settings, role topology and the initial memory snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SnapshotBinding {
    pub source_commit: String,
    pub prompt_version: String,
    pub policy_version: String,
    pub superpod_commit: String,
    pub configuration_digest: String,
}

impl SnapshotBinding {
    pub fn validate(&self) -> Result<()> {
        for (field, value) in [
            ("source commit", &self.source_commit),
            ("prompt version", &self.prompt_version),
            ("policy version", &self.policy_version),
            ("SuperPOD commit", &self.superpod_commit),
            ("configuration digest", &self.configuration_digest),
        ] {
            nonempty(value, field)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DatasetPartition {
    Development,
    Validation,
    Holdout,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Variant {
    Baseline,
    Candidate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrialObservation {
    /// Identifies one identical input and seed executed for both variants.
    pub trial_id: String,
    pub task_family: String,
    pub partition: DatasetPartition,
    pub variant: Variant,
    pub snapshot: SnapshotBinding,
    pub success: bool,
    pub duration_ms: u64,
    /// None means unavailable. Missing values cannot support token improvement.
    pub tokens: Option<u64>,
    pub critical_regression: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationContext {
    pub baseline: SnapshotBinding,
    pub candidate: SnapshotBinding,
}

impl EvaluationContext {
    fn validate(&self) -> Result<()> {
        self.baseline.validate()?;
        self.candidate.validate()?;
        ensure!(
            self.baseline != self.candidate,
            "candidate equals baseline snapshot"
        );
        ensure!(
            self.baseline.policy_version == self.candidate.policy_version,
            "candidate cannot change the formal policy used to evaluate itself"
        );
        ensure!(
            self.baseline.superpod_commit == self.candidate.superpod_commit,
            "paired experiments must use the same SuperPOD snapshot"
        );
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EvaluationMetrics {
    pub families: usize,
    pub pairs: usize,
    pub baseline_successes: usize,
    pub candidate_successes: usize,
    pub baseline_success_rate: f64,
    pub candidate_success_rate: f64,
    pub baseline_median_duration_ms: f64,
    pub candidate_median_duration_ms: f64,
    pub baseline_median_tokens: Option<f64>,
    pub candidate_median_tokens: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationDecision {
    pub approved: bool,
    pub reasons: Vec<String>,
    pub metrics: EvaluationMetrics,
    pub evidence_digest: String,
}

struct Pair<'a> {
    baseline: &'a TrialObservation,
    candidate: &'a TrialObservation,
}

type Families<'a> = BTreeMap<&'a str, Vec<Pair<'a>>>;

fn paired_trials<'a>(
    context: &EvaluationContext,
    observations: &'a [TrialObservation],
) -> Result<Families<'a>> {
    context.validate()?;
    let mut paired =
        BTreeMap::<(&str, &str), (Option<&TrialObservation>, Option<&TrialObservation>)>::new();
    let mut trial_families = BTreeMap::new();
    for observation in observations {
        nonempty(&observation.trial_id, "trial ID")?;
        nonempty(&observation.task_family, "task family")?;
        ensure!(
            observation.partition == DatasetPartition::Holdout,
            "promotion evidence must come from held-out tasks"
        );
        if let Some(family) = trial_families.insert(&observation.trial_id, &observation.task_family)
        {
            ensure!(
                family == &observation.task_family,
                "trial ID reused across task families"
            );
        }
        let expected = match observation.variant {
            Variant::Baseline => &context.baseline,
            Variant::Candidate => &context.candidate,
        };
        ensure!(
            &observation.snapshot == expected,
            "stale or mismatched evidence snapshot"
        );
        let entry = paired
            .entry((&observation.task_family, &observation.trial_id))
            .or_default();
        let slot = match observation.variant {
            Variant::Baseline => &mut entry.0,
            Variant::Candidate => &mut entry.1,
        };
        ensure!(slot.is_none(), "duplicate trial observation");
        *slot = Some(observation);
    }
    let mut families = BTreeMap::<&str, Vec<Pair<'a>>>::new();
    for ((family, _), (baseline, candidate)) in paired {
        families.entry(family).or_default().push(Pair {
            baseline: baseline.context("missing baseline trial partner")?,
            candidate: candidate.context("missing candidate trial partner")?,
        });
    }
    Ok(families)
}

fn median(values: impl Iterator<Item = u64>) -> Option<f64> {
    let mut values: Vec<u64> = values.collect();
    if values.is_empty() {
        return None;
    }
    values.sort_unstable();
    let middle = values.len() / 2;
    if values.len().is_multiple_of(2) {
        Some(values[middle - 1] as f64 / 2.0 + values[middle] as f64 / 2.0)
    } else {
        Some(values[middle] as f64)
    }
}

fn summarize(families: &Families<'_>) -> EvaluationMetrics {
    let pairs: Vec<_> = families.values().flatten().collect();
    let baseline_successes = pairs.iter().filter(|p| p.baseline.success).count();
    let candidate_successes = pairs.iter().filter(|p| p.candidate.success).count();
    let complete_tokens = !pairs.is_empty()
        && pairs
            .iter()
            .all(|p| p.baseline.tokens.is_some() && p.candidate.tokens.is_some());
    EvaluationMetrics {
        families: families.len(),
        pairs: pairs.len(),
        baseline_successes,
        candidate_successes,
        baseline_success_rate: if pairs.is_empty() {
            0.0
        } else {
            baseline_successes as f64 / pairs.len() as f64
        },
        candidate_success_rate: if pairs.is_empty() {
            0.0
        } else {
            candidate_successes as f64 / pairs.len() as f64
        },
        baseline_median_duration_ms: median(pairs.iter().map(|p| p.baseline.duration_ms))
            .unwrap_or(0.0),
        candidate_median_duration_ms: median(pairs.iter().map(|p| p.candidate.duration_ms))
            .unwrap_or(0.0),
        baseline_median_tokens: complete_tokens
            .then(|| median(pairs.iter().filter_map(|p| p.baseline.tokens)))
            .flatten(),
        candidate_median_tokens: complete_tokens
            .then(|| median(pairs.iter().filter_map(|p| p.candidate.tokens)))
            .flatten(),
    }
}

fn improved_efficiency(metrics: &EvaluationMetrics) -> bool {
    let time = metrics.baseline_median_duration_ms > 0.0
        && metrics.candidate_median_duration_ms <= metrics.baseline_median_duration_ms * 0.9;
    let tokens = matches!(
        (metrics.baseline_median_tokens, metrics.candidate_median_tokens),
        (Some(baseline), Some(candidate)) if baseline > 0.0 && candidate <= baseline * 0.9
    );
    metrics.candidate_successes >= metrics.baseline_successes && (time || tokens)
}

fn critical_regression(families: &Families<'_>) -> bool {
    families
        .values()
        .flatten()
        .any(|pair| pair.candidate.critical_regression)
}

/// Malformed, duplicated, unpaired or stale evidence is an error. Valid but
/// insufficient or unsuccessful evidence produces a denied decision.
pub fn evaluate_promotion(
    context: &EvaluationContext,
    observations: &[TrialObservation],
) -> Result<EvaluationDecision> {
    let families = paired_trials(context, observations)?;
    let metrics = summarize(&families);
    let mut reasons = Vec::new();
    if families.len() < 3 || families.values().any(|pairs| pairs.len() < 3) {
        reasons.push("require at least three task families with three paired trials each".into());
    }
    if critical_regression(&families) {
        reasons.push("candidate has a critical regression".into());
    }
    let success_improved = metrics.candidate_successes > metrics.baseline_successes
        && (metrics.candidate_successes - metrics.baseline_successes) as u128 * 10
            >= metrics.pairs as u128;
    if !success_improved && !improved_efficiency(&metrics) {
        reasons.push("require +10 percentage points success or nondecreasing success with 10% median cost improvement".into());
    }
    Ok(EvaluationDecision {
        approved: reasons.is_empty(),
        reasons,
        metrics,
        evidence_digest: digest(&(context, observations))?,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AblationStudy {
    pub removed_component: String,
    pub ablated_snapshot: SnapshotBinding,
    /// Baseline is the ablated system; candidate is the complete candidate.
    pub observations: Vec<TrialObservation>,
}

/// Novelty requires held-out-family transfer and a measured ablation. This
/// records reproducible evidence, not a claim of statistical significance.
pub fn evaluate_novel_capability(
    context: &EvaluationContext,
    observations: &[TrialObservation],
    ablations: &[AblationStudy],
) -> Result<EvaluationDecision> {
    let families = paired_trials(context, observations)?;
    let metrics = summarize(&families);
    let mut reasons = Vec::new();
    if families.len() < 2 || families.values().any(|pairs| pairs.len() != 3) {
        reasons.push("novel capability requires exactly three paired trials in each of at least two held-out families".into());
    }
    if families.values().any(|pairs| {
        pairs.iter().filter(|p| p.candidate.success).count() < 2
            || pairs.iter().filter(|p| p.baseline.success).count() > 1
    }) {
        reasons
            .push("each family requires candidate success >= 2 and baseline success <= 1".into());
    }
    if critical_regression(&families) {
        reasons.push("candidate has a critical regression".into());
    }
    let mut components = BTreeSet::new();
    let mut demonstrated_contribution = false;
    for ablation in ablations {
        nonempty(&ablation.removed_component, "ablated component")?;
        ensure!(
            components.insert(&ablation.removed_component),
            "duplicate ablation component"
        );
        let ablation_context = EvaluationContext {
            baseline: ablation.ablated_snapshot.clone(),
            candidate: context.candidate.clone(),
        };
        let ablated_families = paired_trials(&ablation_context, &ablation.observations)?;
        ensure!(
            ablated_families
                .keys()
                .all(|family| families.contains_key(family)),
            "ablation must test the same held-out task families"
        );
        let summary = summarize(&ablated_families);
        if summary.pairs >= 3
            && !critical_regression(&ablated_families)
            && (summary.candidate_successes > summary.baseline_successes
                || improved_efficiency(&summary))
        {
            demonstrated_contribution = true;
        }
    }
    if !demonstrated_contribution {
        reasons.push("require a paired ablation demonstrating a component's contribution with at least three trials".into());
    }
    Ok(EvaluationDecision {
        approved: reasons.is_empty(),
        reasons,
        metrics,
        evidence_digest: digest(&(context, observations, ablations))?,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplorationOutcome {
    pub experiment_id: String,
    pub promoted_on_holdout: bool,
    pub critical_regression: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplorationBudget {
    share_percent: u8,
    pending: Vec<ExplorationOutcome>,
    completed_experiments: BTreeSet<String>,
}

impl Default for ExplorationBudget {
    fn default() -> Self {
        Self {
            share_percent: 25,
            pending: Vec::new(),
            completed_experiments: BTreeSet::new(),
        }
    }
}

impl ExplorationBudget {
    pub fn share_percent(&self) -> u8 {
        self.share_percent
    }

    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            (10..=50).contains(&self.share_percent),
            "exploration share is outside 10..=50"
        );
        ensure!(self.pending.len() < 10, "unprocessed exploration window");
        let mut seen = BTreeSet::new();
        for outcome in &self.pending {
            nonempty(&outcome.experiment_id, "experiment ID")?;
            ensure!(
                seen.insert(&outcome.experiment_id),
                "duplicate pending experiment"
            );
            ensure!(
                self.completed_experiments.contains(&outcome.experiment_id),
                "unregistered experiment in exploration window"
            );
        }
        Ok(())
    }

    /// Returns Some(new_share) only when a complete window has been processed.
    /// Experiment IDs survive restarts and cannot be counted twice.
    pub fn record(&mut self, outcome: ExplorationOutcome) -> Result<Option<u8>> {
        self.validate()?;
        nonempty(&outcome.experiment_id, "experiment ID")?;
        ensure!(
            !self.completed_experiments.contains(&outcome.experiment_id),
            "experiment already counted"
        );
        self.completed_experiments
            .insert(outcome.experiment_id.clone());
        self.pending.push(outcome);
        if self.pending.len() < 10 {
            return Ok(None);
        }
        let promotions = self
            .pending
            .iter()
            .filter(|entry| entry.promoted_on_holdout)
            .count();
        let critical = self.pending.iter().any(|entry| entry.critical_regression);
        if critical || promotions == 0 {
            self.share_percent = self.share_percent.saturating_sub(5).max(10);
        } else if promotions >= 2 {
            self.share_percent = (self.share_percent + 5).min(50);
        }
        self.pending.clear();
        Ok(Some(self.share_percent))
    }
}

/// A proposed rule is permanently quarantined here. There is intentionally no
/// method to adopt it as formal policy. Adoption requires the repository review
/// and release path outside the self-improvement experiment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuarantinedRuleProposal {
    pub proposal_id: String,
    pub parent_policy_version: String,
    pub proposed_rule: String,
    pub reason: String,
}

impl QuarantinedRuleProposal {
    pub fn new(
        parent_policy_version: String,
        proposed_rule: String,
        reason: String,
    ) -> Result<Self> {
        nonempty(&parent_policy_version, "parent policy version")?;
        nonempty(&proposed_rule, "proposed rule")?;
        nonempty(&reason, "rule change reason")?;
        let proposal_id = digest(&(&parent_policy_version, &proposed_rule, &reason))?;
        Ok(Self {
            proposal_id,
            parent_policy_version,
            proposed_rule,
            reason,
        })
    }

    pub fn validate(&self) -> Result<()> {
        let rebuilt = Self::new(
            self.parent_policy_version.clone(),
            self.proposed_rule.clone(),
            self.reason.clone(),
        )?;
        ensure!(
            rebuilt.proposal_id == self.proposal_id,
            "rule proposal hash mismatch"
        );
        Ok(())
    }

    pub fn can_adopt_automatically(&self) -> bool {
        false
    }
}

/// All paths must exist. Canonicalization detects symlinks that would otherwise
/// leak held-out tasks through a seemingly separate workspace or shared store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HoldoutIsolation {
    pub private_root: PathBuf,
    pub optimizer_roots: Vec<PathBuf>,
    pub shared_knowledge_roots: Vec<PathBuf>,
    pub shared_memory_roots: Vec<PathBuf>,
}

impl HoldoutIsolation {
    pub fn validate(&self) -> Result<()> {
        let private = canonical_directory(&self.private_root)?;
        ensure!(
            !self.optimizer_roots.is_empty(),
            "optimizer must have an explicit workspace"
        );
        for path in self
            .optimizer_roots
            .iter()
            .chain(&self.shared_knowledge_roots)
            .chain(&self.shared_memory_roots)
        {
            let exposed = canonical_directory(path)?;
            ensure!(
                !private.starts_with(&exposed) && !exposed.starts_with(&private),
                "holdout directory overlaps an agent workspace or shared store"
            );
            // Inspect nested symlinks as well; the root itself can be safe while
            // a descendant exposes the protected tree.
            reject_holdout_links(&exposed, &private)?;
        }
        Ok(())
    }

    pub fn authorize_optimizer_path(&self, path: &Path) -> Result<()> {
        self.validate()?;
        let resolved = path
            .canonicalize()
            .with_context(|| format!("cannot resolve {}", path.display()))?;
        let private = self.private_root.canonicalize()?;
        ensure!(
            !resolved.starts_with(&private),
            "optimizer cannot access held-out tasks or answers"
        );
        for root in &self.optimizer_roots {
            if resolved.starts_with(root.canonicalize()?) {
                return Ok(());
            }
        }
        bail!("optimizer path is outside its declared workspaces")
    }
}

fn canonical_directory(path: &Path) -> Result<PathBuf> {
    let canonical = path
        .canonicalize()
        .with_context(|| format!("cannot resolve {}", path.display()))?;
    ensure!(canonical.is_dir(), "{} must be a directory", path.display());
    Ok(canonical)
}

fn reject_holdout_links(exposed: &Path, private: &Path) -> Result<()> {
    let mut pending = vec![exposed.to_path_buf()];
    let mut visited = BTreeSet::new();
    while let Some(directory) = pending.pop() {
        if !visited.insert(directory.clone()) {
            continue;
        }
        for entry in std::fs::read_dir(&directory)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if file_type.is_symlink() {
                let target = entry
                    .path()
                    .canonicalize()
                    .context("unresolvable symlink in exposed workspace")?;
                ensure!(
                    !target.starts_with(private) && !private.starts_with(&target),
                    "nested symlink exposes held-out tasks"
                );
                // Directory links outside the declared roots may lead back to
                // the private tree. Reject them rather than traversing the host.
                if target.is_dir() {
                    ensure!(
                        target.starts_with(exposed),
                        "directory symlink escapes declared workspace"
                    );
                    pending.push(target);
                }
            } else if file_type.is_dir() {
                pending.push(entry.path());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
