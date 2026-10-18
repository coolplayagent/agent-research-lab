use contracts::{FollowupAdmission, MemoryWriteback};
use serde::{Deserialize, Serialize};
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PostSuccess {
    pub schema_version: u32,
    pub task_id: String,
    pub run_id: String,
    pub receipt_sha256: String,
    pub memory: MemoryWriteback,
    pub persona_memory: MemoryWriteback,
    pub persona_memory_error: Option<String>,
    pub followups: FollowupAdmission,
    pub memory_error: Option<String>,
    pub followup_error: Option<String>,
    pub followup_queued: Vec<String>,
    pub followup_rejections: Vec<ProposalRejection>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProposalRejection {
    pub index: usize,
    pub reason: String,
}
