//! Frozen pilot records and claim/evaluation wire data; neither authorizes promotion.
use evolution::PromptVersion;
use inputs::ExecutionInputs;
use serde::{Deserialize, Serialize};
use task::Task;

pub(super) const SLOTS: usize = 8;
pub(super) const LEGACY_PROTOCOL: &str = "collaboration-pilot-v1";
pub(super) const PROTOCOL: &str = "collaboration-pilot-v2";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Slot {
    pub id: String,
    pub role: String,
    pub model: String,
    pub perspective: String,
    /// Optional registered ancestor; candidates are never activated by this module.
    pub parent_prompt_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanRequest {
    pub schema_version: u32,
    pub experiment_id: String,
    pub task_family: String,
    pub trial_id: String,
    pub question: String,
    pub primary_hypothesis: String,
    pub policy_version: String,
    pub max_claims: usize,
    pub task_timeout_seconds: u64,
    pub first_arm: Arm,
    pub slots: Vec<Slot>,
    pub knowledge_sources: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Arm {
    Baseline,
    Candidate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlannedTask {
    pub arm: Arm,
    pub round: u8,
    pub slot: String,
    pub model: String,
    pub task: Task,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub schema_version: u32,
    pub protocol: String,
    pub request: PlanRequest,
    pub config_digest: String,
    pub input_digest: String,
    pub inputs: ExecutionInputs,
    pub prompts: Vec<PromptVersion>,
    pub tasks: Vec<PlannedTask>,
    pub digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Citation {
    pub repository: String,
    pub commit: String,
    pub path: String,
    pub start_line: usize,
    pub end_line: usize,
    /// SHA-256 of the complete file at the exact commit, not a mutable worktree.
    pub sha256: String,
    pub quote: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimCard {
    pub id: String,
    pub text: String,
    pub evidence: Vec<Citation>,
    /// Qualified IDs use task-id#claim-id; only supplied dependency cards may be cited.
    pub counterexample_to: Vec<String>,
    pub revises: Vec<String>,
    pub change: Change,
    pub rationale: String,
    pub knowledge_update: Option<KnowledgeUpdate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeUpdate {
    pub target_path: String,
    pub proposal: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Change {
    New,
    Keep,
    Narrow,
    Retract,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Adjudication {
    pub claim_id: String,
    pub claim_digest: String,
    pub supported: bool,
    pub effective_counterexample: bool,
    pub valid_revision: bool,
    /// Independent comparison with the initial round; None means unassessed.
    pub introduced_error: Option<bool>,
    pub duplicate_of: Option<String>,
    pub rationale: String,
    pub evidence: Vec<Citation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Adjudications {
    pub schema_version: u32,
    pub plan_digest: String,
    pub entries: Vec<Adjudication>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectedClaim {
    pub id: String,
    pub digest: String,
    pub arm: Arm,
    pub round: u8,
    pub card: ClaimCard,
    pub citation_errors: Vec<String>,
    pub adjudication: Option<Adjudication>,
}
