//! Evidence bindings, paired observations and advisory evaluation records.
use crate::identity::nonempty;
use anyhow::{Result, ensure};
use contracts::{DatasetPartition, TrialVariant as Variant};
use serde::{Deserialize, Serialize};

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
    pub(super) fn validate(&self) -> Result<()> {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AblationStudy {
    pub removed_component: String,
    pub ablated_snapshot: SnapshotBinding,
    /// Baseline is the ablated system; candidate is the complete candidate.
    pub observations: Vec<TrialObservation>,
}
