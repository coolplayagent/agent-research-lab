//! Bounded collaboration metadata. Knowledge contents remain in optional external stores.
use crate::EvidencePins;
use contracts::{
    KnowledgeWriteState, OutcomeAcceptanceState, OutcomeKind, OutcomePublicationState,
    OutcomeReferenceKind, OutcomeVerificationState, VerificationRunState,
};
use serde::{Deserialize, Serialize};

pub const OUTCOME_PROTOCOL_VERSION: u32 = 1;
pub const OUTCOME_PAGE_SIZE: usize = 32;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutcomePath {
    pub topic_id: String,
    pub goal_id: String,
    pub name: String,
    pub hypothesis: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutcomeAttempt {
    pub goal_id: String,
    pub work_id: String,
    pub attempt: u32,
    pub claim_id: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutcomeEvidence {
    pub repository: String,
    pub commit: String,
    pub path: String,
    pub sha256: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutcomeReference {
    pub outcome_id: String,
    pub kind: OutcomeReferenceKind,
    pub reason: String,
}

/// An optional external source binding, frozen with each published version.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutcomeKnowledgeBinding {
    pub service_id: String,
    pub repository: String,
    pub revision: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutcomeInput {
    pub id: String,
    pub topic_id: String,
    pub goal_id: String,
    pub kind: OutcomeKind,
    pub title: String,
    pub summary: String,
    pub conditions: String,
    pub evidence: Vec<OutcomeEvidence>,
    pub references: Vec<OutcomeReference>,
    pub supersedes: Option<String>,
    pub attempt: Option<OutcomeAttempt>,
    pub pins: EvidencePins,
    pub knowledge: Option<OutcomeKnowledgeBinding>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Outcome {
    pub input: OutcomeInput,
    pub author_id: String,
    pub created_ms: u64,
    pub digest: String,
    pub sequence: u64,
}
impl Outcome {
    pub fn validate_digest(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.digest == crate::digest(&serde_json::to_vec(&(&self.author_id, &self.input))?),
            "outcome content digest mismatch"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutcomeQuery {
    pub topic_id: String,
    #[serde(default)]
    pub goal_id: String,
    #[serde(default)]
    pub query: String,
    #[serde(default)]
    pub after: u64,
    #[serde(default)]
    pub snapshot: Option<u64>,
    #[serde(default)]
    pub kind: Option<OutcomeKind>,
}

/// The service configuration digest and outcome digest bind a remote operation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutcomeVerificationInput {
    pub id: String,
    pub outcome_id: String,
    pub outcome_digest: String,
    pub verifier_id: String,
    pub verifier_digest: String,
    pub max_seconds: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutcomeVerificationResult {
    pub id: String,
    pub request_digest: String,
    pub state: VerificationRunState,
    pub summary: String,
    pub evidence: Vec<OutcomeEvidence>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutcomeVerification {
    pub input: OutcomeVerificationInput,
    pub request_digest: String,
    pub created_ms: u64,
    pub deadline_ms: u64,
    pub result: Option<OutcomeVerificationResult>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutcomeDecision {
    pub id: String,
    pub outcome_id: String,
    pub verification_id: Option<String>,
    pub state: OutcomeAcceptanceState,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeWriteInput {
    pub id: String,
    pub outcome_id: String,
    pub outcome_digest: String,
    pub acceptance_id: String,
    pub service_id: String,
    pub service_digest: String,
    pub repository: String,
    pub expected_revision: String,
    pub max_seconds: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeWriteResult {
    pub id: String,
    pub request_digest: String,
    pub state: KnowledgeWriteState,
    pub reference: String,
    pub revision: String,
    pub summary: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeWrite {
    pub input: KnowledgeWriteInput,
    pub request_digest: String,
    pub created_ms: u64,
    pub deadline_ms: u64,
    pub result: Option<KnowledgeWriteResult>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutcomeView {
    pub outcome: Outcome,
    pub publication: OutcomePublicationState,
    pub withdrawal_reason: Option<String>,
    pub verification: OutcomeVerificationState,
    pub acceptance: OutcomeAcceptanceState,
    pub latest_verification: Option<OutcomeVerification>,
    pub latest_decision: Option<OutcomeDecision>,
    pub latest_write: Option<KnowledgeWrite>,
    pub reference_count: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum OutcomeCommand {
    LinkPath { path: OutcomePath },
    Paths { topic_id: String, after: String },
    Publish { input: Box<OutcomeInput> },
    Search { query: OutcomeQuery },
    Get { id: String },
    Withdraw { id: String, reason: String },
    BeginVerification { input: OutcomeVerificationInput },
    FinishVerification { result: OutcomeVerificationResult },
    Decide { decision: OutcomeDecision },
    BeginWrite { input: KnowledgeWriteInput },
    FinishWrite { result: KnowledgeWriteResult },
    History { id: String, after: u64 },
}

impl OutcomeCommand {
    pub fn agent_allowed(&self) -> bool {
        matches!(
            self,
            Self::Publish { .. }
                | Self::Search { .. }
                | Self::Get { .. }
                | Self::Paths { .. }
                | Self::History { .. }
        )
    }
    pub fn is_read(&self) -> bool {
        matches!(
            self,
            Self::Search { .. } | Self::Get { .. } | Self::Paths { .. } | Self::History { .. }
        )
    }
}
