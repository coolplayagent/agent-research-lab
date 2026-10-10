//! HGM-inspired clade diagnostics over the existing immutable prompt archive.
//! This report neither samples a scheduler policy nor authorizes promotion.
use super::*;
const MAX_VERSIONS: usize = 2048;
const MAX_OBSERVATIONS: usize = 100_000;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Study {
    pub task_kind: String,
    pub policy_version: String,
    pub superpod_commit: String,
    /// Host-frozen expected bindings, independent of the reported observations.
    pub expected: BTreeMap<String, SnapshotBinding>,
    pub observations: Vec<TrialObservation>,
    /// In-flight jobs remain visible but cannot count as successful evidence.
    #[serde(default)]
    pub pending_evaluations: BTreeMap<String, u32>,
    #[serde(default)]
    pub pending_expansions: BTreeMap<String, u32>,
    pub evaluation_budget: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub version: String,
    pub parents: Vec<String>,
    pub role: String,
    pub change_reason: String,
    pub trials: usize,
    pub successes: usize,
    pub critical_regressions: usize,
    pub descendant_versions: usize,
    pub clade_trials: usize,
    pub clade_successes: usize,
    pub clade_success_rate: Option<f64>,
    pub clade_posterior_mean: Option<f64>,
    pub duration_ms: u64,
    pub tokens: Option<u64>,
    pub pending_evaluations: u32,
    pub pending_expansions: u32,
    pub source: Option<SnapshotBinding>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    pub schema_version: u32,
    pub task_kind: String,
    pub policy_version: String,
    pub superpod_commit: String,
    pub evidence_digest: String,
    pub archive_digest: String,
    pub nodes: Vec<Node>,
    pub completed_evaluations: usize,
    pub pending_evaluations: u64,
    pub pending_expansions: u64,
    pub evaluation_budget: u64,
    pub budget_remaining: u64,
    pub development_only: bool,
    pub promotion_authorized: bool,
    pub interpretation: String,
}
fn hex(value: &str, length: usize) -> bool {
    value.len() == length && value.bytes().all(|b| b.is_ascii_hexdigit())
}
pub fn diagnose(registry: &PromptRegistry, study: &Study) -> Result<Report> {
    ensure!(
        registry.versions.len() <= MAX_VERSIONS,
        "lineage archive exceeds diagnostic bound"
    );
    registry.validate()?;
    nonempty(&study.task_kind, "task kind")?;
    ensure!(
        hex(&study.superpod_commit, 40),
        "SuperPOD commit must be pinned"
    );
    ensure!(
        hex(&study.policy_version, 40) || hex(&study.policy_version, 64),
        "policy must be pinned to a commit or content digest"
    );
    ensure!(
        study.observations.len() <= MAX_OBSERVATIONS,
        "observation bound exceeded"
    );
    ensure!(
        study.evaluation_budget > 0 && study.evaluation_budget <= 1_000_000,
        "evaluation budget exceeds bound"
    );
    let versions: BTreeSet<_> = registry
        .versions
        .iter()
        .filter(|(_, p)| p.definition.task_kind == study.task_kind)
        .map(|(id, _)| id.clone())
        .collect();
    ensure!(!versions.is_empty(), "no strategies for this task kind");
    for (version, binding) in &study.expected {
        binding.validate()?;
        ensure!(
            versions.contains(version) && binding.prompt_version == *version,
            "unknown or mismatched expected prompt"
        );
        ensure!(
            hex(&binding.source_commit, 40) && hex(&binding.configuration_digest, 64),
            "source and configuration must be pinned"
        );
        ensure!(
            binding.policy_version == study.policy_version
                && binding.superpod_commit == study.superpod_commit,
            "incomparable evidence policy or knowledge baseline"
        );
    }
    for pending in [&study.pending_evaluations, &study.pending_expansions] {
        ensure!(
            pending.keys().all(|v| versions.contains(v)),
            "pending job refers to unknown strategy"
        );
        ensure!(
            pending.values().map(|n| u64::from(*n)).sum::<u64>() <= 1_000_000,
            "pending job count exceeds bound"
        );
    }
    let mut observations = BTreeMap::<&str, Vec<&TrialObservation>>::new();
    let mut seen = BTreeSet::new();
    for observation in &study.observations {
        ensure!(
            observation.partition == DatasetPartition::Development,
            "lineage optimization cannot consume validation or private holdout evidence"
        );
        let version = observation.snapshot.prompt_version.as_str();
        let expected = study
            .expected
            .get(version)
            .context("observation lacks host-frozen binding")?;
        ensure!(
            &observation.snapshot == expected,
            "observation changed source/prompt/policy/knowledge/configuration binding"
        );
        nonempty(&observation.trial_id, "trial ID")?;
        nonempty(&observation.task_family, "task family")?;
        ensure!(
            seen.insert((version, &observation.task_family, &observation.trial_id)),
            "duplicate trial would inflate clade evidence"
        );
        observations.entry(version).or_default().push(observation);
    }
    let pending_evaluations = study
        .pending_evaluations
        .values()
        .map(|v| u64::from(*v))
        .sum::<u64>();
    let pending_expansions = study
        .pending_expansions
        .values()
        .map(|v| u64::from(*v))
        .sum::<u64>();
    let used = (study.observations.len() as u64)
        .checked_add(pending_evaluations)
        .context("budget overflow")?;
    ensure!(
        used <= study.evaluation_budget,
        "completed and pending evaluations exceed budget"
    );
    let mut children = BTreeMap::<&str, Vec<&str>>::new();
    for version in &versions {
        for parent in &registry.versions[version].definition.parent_versions {
            children.entry(parent).or_default().push(version);
        }
    }
    let mut nodes = Vec::with_capacity(versions.len());
    for version in &versions {
        let mut clade = BTreeSet::new();
        let mut visit = vec![version.as_str()];
        while let Some(id) = visit.pop() {
            if clade.insert(id) {
                visit.extend(children.get(id).into_iter().flatten().copied());
            }
        }
        let own = observations
            .get(version.as_str())
            .cloned()
            .unwrap_or_default();
        let trials = clade
            .iter()
            .flat_map(|id| observations.get(id).into_iter().flatten().copied())
            .collect::<Vec<_>>();
        let successes = trials
            .iter()
            .filter(|t| t.success && !t.critical_regression)
            .count();
        let prompt = &registry.versions[version].definition;
        let duration_ms = own.iter().try_fold(0u64, |s, t| {
            s.checked_add(t.duration_ms).context("duration overflow")
        })?;
        let tokens = if own.is_empty() || own.iter().any(|t| t.tokens.is_none()) {
            None
        } else {
            Some(own.iter().try_fold(0u64, |s, t| {
                s.checked_add(t.tokens.unwrap())
                    .context("token count overflow")
            })?)
        };
        nodes.push(Node {
            version: version.clone(),
            parents: prompt.parent_versions.clone(),
            role: prompt.role.clone(),
            change_reason: prompt.change_reason.chars().take(1000).collect(),
            trials: own.len(),
            successes: own
                .iter()
                .filter(|t| t.success && !t.critical_regression)
                .count(),
            critical_regressions: own.iter().filter(|t| t.critical_regression).count(),
            descendant_versions: clade.len() - 1,
            clade_trials: trials.len(),
            clade_successes: successes,
            clade_success_rate: (!trials.is_empty())
                .then(|| successes as f64 / trials.len() as f64),
            clade_posterior_mean: (!trials.is_empty())
                .then(|| (successes + 1) as f64 / (trials.len() + 2) as f64),
            duration_ms,
            tokens,
            pending_evaluations: study.pending_evaluations.get(version).copied().unwrap_or(0),
            pending_expansions: study.pending_expansions.get(version).copied().unwrap_or(0),
            source: study.expected.get(version).cloned(),
        });
    }
    Ok(Report{schema_version:1,task_kind:study.task_kind.clone(),policy_version:study.policy_version.clone(),superpod_commit:study.superpod_commit.clone(),evidence_digest:digest(study)?,archive_digest:digest(registry)?,nodes,completed_evaluations:study.observations.len(),pending_evaluations,pending_expansions,evaluation_budget:study.evaluation_budget,budget_remaining:study.evaluation_budget-used,development_only:true,promotion_authorized:false,interpretation:"HGM-inspired clade diagnostics; shared descendants counted once per clade. Beta(1,1) posterior mean is a descriptive estimate, not Thompson scheduling, an independent confidence guarantee, or promotion approval. Digital-person identity and private memories do not mutate with a strategy version.".into()})
}
#[cfg(test)]
mod tests {
    use super::*;
    fn register(r: &mut PromptRegistry, name: &str, parents: Vec<String>) -> String {
        r.register(
            PromptVersion::new(PromptDefinition {
                parent_versions: parents,
                role: "research".into(),
                task_kind: "coding".into(),
                content: name.into(),
                change_reason: name.into(),
                failure_conditions: vec!["regression".into()],
            })
            .unwrap(),
        )
        .unwrap()
    }
    fn binding(version: &str) -> SnapshotBinding {
        SnapshotBinding {
            source_commit: "a".repeat(40),
            prompt_version: version.into(),
            policy_version: "b".repeat(40),
            superpod_commit: "c".repeat(40),
            configuration_digest: "d".repeat(64),
        }
    }
    #[test]
    fn descendant_evidence_survives_recombination_without_double_counting_or_promotion() {
        let mut registry = PromptRegistry::default();
        let a = register(&mut registry, "a", vec![]);
        let b = register(&mut registry, "b", vec![a.clone()]);
        let c = register(&mut registry, "c", vec![a.clone()]);
        let d = register(&mut registry, "d", vec![b.clone(), c.clone()]);
        let observation = TrialObservation {
            trial_id: "shared-case".into(),
            task_family: "family".into(),
            partition: DatasetPartition::Development,
            variant: Variant::Candidate,
            snapshot: binding(&d),
            success: true,
            duration_ms: 10,
            tokens: None,
            critical_regression: false,
        };
        let mut study = Study {
            task_kind: "coding".into(),
            policy_version: "b".repeat(40),
            superpod_commit: "c".repeat(40),
            expected: BTreeMap::from([(d.clone(), binding(&d))]),
            observations: vec![observation],
            pending_evaluations: BTreeMap::from([(b.clone(), 1)]),
            pending_expansions: BTreeMap::new(),
            evaluation_budget: 10,
        };
        let report = diagnose(&registry, &study).unwrap();
        let root = report.nodes.iter().find(|n| n.version == a).unwrap();
        assert_eq!(root.clade_trials, 1);
        assert_eq!(root.descendant_versions, 3);
        assert_eq!(root.trials, 0);
        assert_eq!(root.tokens, None);
        assert!(!report.promotion_authorized);
        assert_eq!(report.budget_remaining, 8);
        study.observations.push(study.observations[0].clone());
        assert!(diagnose(&registry, &study).is_err());
        study.observations.pop();
        study.observations[0].partition = DatasetPartition::Holdout;
        assert!(diagnose(&registry, &study).is_err());
        study.observations[0].partition = DatasetPartition::Development;
        study.observations[0].snapshot.source_commit = "e".repeat(40);
        assert!(diagnose(&registry, &study).is_err());
    }
}
