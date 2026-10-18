//! Strict host command inputs; candidate data never supplies an approval decision.
use delivery::VerificationArtifact;
use evolution::{
    AblationStudy, HoldoutIsolation, PromptDefinition, StrategyPortfolio, TrialObservation,
};
use freshness::RemoteSnapshot;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
