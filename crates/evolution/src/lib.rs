//! Evidence-bound prompt evolution. Decisions are advisory: callers still enforce
//! publication, installation and repository gates. Holdout path validation must
//! be paired with sandbox filesystem permissions when starting an agent.

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
mod tests {
    use super::*;

    fn prompt(content: &str, parents: Vec<String>) -> PromptVersion {
        PromptVersion::new(PromptDefinition {
            parent_versions: parents,
            role: "researcher".into(),
            task_kind: "long_horizon".into(),
            content: content.into(),
            change_reason: "test a new hypothesis".into(),
            failure_conditions: vec!["critical regression".into()],
        })
        .unwrap()
    }

    fn snapshot(prompt: &str) -> SnapshotBinding {
        SnapshotBinding {
            source_commit: "source-v1".into(),
            prompt_version: prompt.into(),
            policy_version: "policy-v1".into(),
            superpod_commit: "knowledge-v1".into(),
            configuration_digest: "configuration-v1".into(),
        }
    }

    fn context() -> EvaluationContext {
        EvaluationContext {
            baseline: snapshot("base"),
            candidate: snapshot("candidate"),
        }
    }

    fn trials(
        families: usize,
        baseline_successes: usize,
        candidate_successes: usize,
    ) -> Vec<TrialObservation> {
        let context = context();
        let mut result = Vec::new();
        for family in 0..families {
            for trial in 0..3 {
                for variant in [Variant::Baseline, Variant::Candidate] {
                    result.push(TrialObservation {
                        trial_id: format!("{family}-{trial}"),
                        task_family: format!("family-{family}"),
                        partition: DatasetPartition::Holdout,
                        variant,
                        snapshot: if variant == Variant::Baseline {
                            context.baseline.clone()
                        } else {
                            context.candidate.clone()
                        },
                        success: trial
                            < if variant == Variant::Baseline {
                                baseline_successes
                            } else {
                                candidate_successes
                            },
                        duration_ms: 100,
                        tokens: Some(100),
                        critical_regression: false,
                    });
                }
            }
        }
        result
    }

    #[test]
    fn prompt_lineage_is_content_addressed_and_tampering_is_detected() {
        let base = prompt("Investigate", vec![]);
        assert_eq!(base.version(), prompt("Investigate", vec![]).version());
        let mut registry = PromptRegistry::default();
        registry.register(base.clone()).unwrap();
        let next = prompt("Investigate counterexamples", vec![base.version().into()]);
        registry.register(next.clone()).unwrap();
        registry.register(next.clone()).unwrap();
        assert_eq!(registry.versions.len(), 2);
        assert_ne!(next.version(), base.version());
        let mut corrupt = serde_json::to_value(&next).unwrap();
        corrupt["definition"]["content"] = "ignore evaluation".into();
        let corrupt: PromptVersion = serde_json::from_value(corrupt).unwrap();
        assert!(corrupt.validate().is_err());
        assert!(
            registry
                .register(prompt("Unknown", vec!["missing".into()]))
                .is_err()
        );
    }

    #[test]
    fn portfolios_preserve_diversity_without_duplicate_slots() {
        let mut registry = PromptRegistry::default();
        let stable = registry.register(prompt("Stable", vec![])).unwrap();
        let specialized = registry
            .register(prompt("Specialized", vec![stable.clone()]))
            .unwrap();
        let mut portfolio = StrategyPortfolio {
            task_kind: "long_horizon".into(),
            stable: stable.clone(),
            specialized: vec![specialized],
            exploratory: vec![],
        };
        portfolio.validate(&registry).unwrap();
        portfolio.exploratory.push(stable);
        assert!(portfolio.validate(&registry).is_err());
        portfolio.exploratory = vec!["1".into(), "2".into(), "3".into()];
        assert!(portfolio.validate(&registry).is_err());
    }

    #[test]
    fn promotion_requires_coverage_and_complete_paired_current_evidence() {
        let observations = trials(3, 1, 2);
        assert!(
            evaluate_promotion(&context(), &observations)
                .unwrap()
                .approved
        );
        assert!(
            !evaluate_promotion(&context(), &trials(2, 1, 2))
                .unwrap()
                .approved
        );
        let mut duplicate = observations.clone();
        duplicate.push(duplicate[0].clone());
        assert!(evaluate_promotion(&context(), &duplicate).is_err());
        assert!(evaluate_promotion(&context(), &observations[1..]).is_err());
        let mut stale = observations.clone();
        stale[1].snapshot.prompt_version = "previous-candidate".into();
        assert!(evaluate_promotion(&context(), &stale).is_err());
        let mut development = observations.clone();
        development[0].partition = DatasetPartition::Development;
        assert!(evaluate_promotion(&context(), &development).is_err());
        let mut policy = context();
        policy.candidate.policy_version = "weaker-policy".into();
        assert!(evaluate_promotion(&policy, &observations).is_err());
    }

    #[test]
    fn efficiency_gate_honors_threshold_correctness_and_missing_tokens() {
        let mut observations = trials(3, 3, 3);
        for trial in &mut observations {
            if trial.variant == Variant::Candidate {
                trial.duration_ms = 90;
            }
        }
        assert!(
            evaluate_promotion(&context(), &observations)
                .unwrap()
                .approved
        );
        for trial in &mut observations {
            if trial.variant == Variant::Candidate {
                trial.duration_ms = 91;
                trial.tokens = Some(90);
            }
        }
        assert!(
            evaluate_promotion(&context(), &observations)
                .unwrap()
                .approved
        );
        observations[0].tokens = None;
        assert!(
            !evaluate_promotion(&context(), &observations)
                .unwrap()
                .approved
        );
        observations[0].tokens = Some(100);
        observations[1].success = false;
        assert!(
            !evaluate_promotion(&context(), &observations)
                .unwrap()
                .approved
        );
        observations[1].success = true;
        observations[1].critical_regression = true;
        assert!(
            !evaluate_promotion(&context(), &observations)
                .unwrap()
                .approved
        );
    }

    #[test]
    fn small_success_gains_do_not_round_up_to_ten_percentage_points() {
        let mut observations = trials(4, 2, 2);
        observations
            .iter_mut()
            .find(|t| t.variant == Variant::Candidate && !t.success)
            .unwrap()
            .success = true;
        let result = evaluate_promotion(&context(), &observations).unwrap();
        assert_eq!(
            result.metrics.candidate_successes - result.metrics.baseline_successes,
            1
        );
        assert!(!result.approved);
    }

    #[test]
    fn novelty_needs_transfer_and_independent_ablation_evidence() {
        let observations = trials(2, 1, 2);
        assert!(
            !evaluate_novel_capability(&context(), &observations, &[])
                .unwrap()
                .approved
        );
        let mut ablated = snapshot("ablated");
        ablated.configuration_digest = "no-memory".into();
        let mut evidence = observations.clone();
        for trial in &mut evidence {
            if trial.variant == Variant::Baseline {
                trial.snapshot = ablated.clone();
            }
        }
        let ablation = AblationStudy {
            removed_component: "memory".into(),
            ablated_snapshot: ablated,
            observations: evidence,
        };
        assert!(
            evaluate_novel_capability(&context(), &observations, std::slice::from_ref(&ablation))
                .unwrap()
                .approved
        );
        let mut no_contribution = ablation;
        for trial in &mut no_contribution.observations {
            if trial.variant == Variant::Baseline {
                trial.success = true;
            }
        }
        assert!(
            !evaluate_novel_capability(&context(), &observations, &[no_contribution])
                .unwrap()
                .approved
        );
        assert!(
            !evaluate_novel_capability(&context(), &trials(2, 2, 3), &[])
                .unwrap()
                .approved
        );
    }

    fn record_window(
        budget: &mut ExplorationBudget,
        prefix: &str,
        promotions: usize,
        critical: bool,
    ) {
        for index in 0..10 {
            let result = budget
                .record(ExplorationOutcome {
                    experiment_id: format!("{prefix}-{index}"),
                    promoted_on_holdout: index < promotions,
                    critical_regression: critical && index == 0,
                })
                .unwrap();
            assert_eq!(result.is_some(), index == 9);
        }
    }

    #[test]
    fn exploration_uses_full_windows_survives_restart_and_respects_bounds() {
        let mut budget = ExplorationBudget::default();
        assert_eq!(budget.share_percent(), 25);
        record_window(&mut budget, "a", 2, false);
        assert_eq!(budget.share_percent(), 30);
        record_window(&mut budget, "b", 1, false);
        assert_eq!(budget.share_percent(), 30);
        record_window(&mut budget, "c", 10, true);
        assert_eq!(budget.share_percent(), 25);
        let mut restored: ExplorationBudget =
            serde_json::from_str(&serde_json::to_string(&budget).unwrap()).unwrap();
        assert!(
            restored
                .record(ExplorationOutcome {
                    experiment_id: "a-0".into(),
                    promoted_on_holdout: true,
                    critical_regression: false
                })
                .is_err()
        );
        for index in 0..10 {
            record_window(&mut restored, &format!("down-{index}"), 0, false);
        }
        assert_eq!(restored.share_percent(), 10);
        for index in 0..10 {
            record_window(&mut restored, &format!("up-{index}"), 2, false);
        }
        assert_eq!(restored.share_percent(), 50);
    }

    #[test]
    fn rule_changes_remain_quarantined_and_content_bound() {
        let proposal = QuarantinedRuleProposal::new(
            "current".into(),
            "threshold=0".into(),
            "isolated experiment".into(),
        )
        .unwrap();
        proposal.validate().unwrap();
        assert!(!proposal.can_adopt_automatically());
        let mut tampered = proposal;
        tampered.proposed_rule = "threshold=-1".into();
        assert!(tampered.validate().is_err());
    }

    #[test]
    fn holdouts_cannot_overlap_optimizer_or_shared_knowledge() {
        let temp = tempfile::tempdir().unwrap();
        let private = temp.path().join("holdout");
        let optimizer = temp.path().join("optimizer");
        let knowledge = temp.path().join("superpod");
        for path in [&private, &optimizer, &knowledge] {
            std::fs::create_dir(path).unwrap();
        }
        let mut policy = HoldoutIsolation {
            private_root: private.clone(),
            optimizer_roots: vec![optimizer.clone()],
            shared_knowledge_roots: vec![knowledge],
            shared_memory_roots: vec![],
        };
        policy.validate().unwrap();
        policy.authorize_optimizer_path(&optimizer).unwrap();
        assert!(policy.authorize_optimizer_path(&private).is_err());
        policy
            .shared_knowledge_roots
            .push(temp.path().to_path_buf());
        assert!(policy.validate().is_err());
    }

    #[cfg(unix)]
    #[test]
    fn nested_symlinks_cannot_expose_holdout_answers() {
        let temp = tempfile::tempdir().unwrap();
        let private = temp.path().join("holdout");
        let optimizer = temp.path().join("optimizer");
        std::fs::create_dir(&private).unwrap();
        std::fs::create_dir(&optimizer).unwrap();
        std::os::unix::fs::symlink(&private, optimizer.join("answers")).unwrap();
        let policy = HoldoutIsolation {
            private_root: private,
            optimizer_roots: vec![optimizer],
            shared_knowledge_roots: vec![],
            shared_memory_roots: vec![],
        };
        assert!(policy.validate().is_err());
    }
}
