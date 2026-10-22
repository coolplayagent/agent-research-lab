//! Durable outcome tasks and versioned coordinator proposals.
pub use contracts::{TaskErrorCode, TaskEventKind, TaskKind, TaskNodeState, TaskState};
use serde::{Deserialize, Serialize};
use serde_json::Value;
pub const COORDINATOR_PROTOCOL_VERSION: u32 = 1;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskBudget {
    pub max_nodes: u32,
    pub max_attempts: u32,
    pub max_revisions: u32,
    pub active_seconds: u64,
    pub execution_seconds: u64,
}
impl Default for TaskBudget {
    fn default() -> Self {
        Self {
            max_nodes: 32,
            max_attempts: 3,
            max_revisions: 3,
            active_seconds: 1800,
            execution_seconds: 300,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskInput {
    pub id: String,
    pub group_id: String,
    pub intent: String,
    #[serde(default)]
    pub budget: TaskBudget,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskNodeInput {
    pub id: String,
    pub person_id: String,
    pub title: String,
    pub instruction: String,
    pub deliverable: String,
    pub acceptance: String,
    pub dependencies: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskArtifact {
    pub id: String,
    pub node_id: String,
    pub attempt: u32,
    pub graph_revision: u64,
    pub author_id: String,
    pub summary: String,
    pub evidence: Value,
    pub inputs: Vec<String>,
    pub digest: String,
    pub created_ms: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskLease {
    pub claim_id: String,
    pub graph_revision: u64,
    pub deadline_ms: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskAttempt {
    pub attempt: u32,
    pub claim_id: String,
    pub started_ms: u64,
    pub finished_ms: Option<u64>,
    pub error: Option<TaskErrorCode>,
    pub receipt: Value,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskNode {
    pub input: TaskNodeInput,
    pub state: TaskNodeState,
    pub attempt: u32,
    pub lease: Option<TaskLease>,
    pub artifact_id: Option<String>,
    pub review: String,
    pub attempts: Vec<TaskAttempt>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskDelivery {
    pub summary: String,
    pub artifact_ids: Vec<String>,
    pub created_ms: u64,
    pub accepted_ms: Option<u64>,
    pub return_reason: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskEvent {
    pub sequence: u64,
    pub kind: TaskEventKind,
    pub node_id: Option<String>,
    pub text: String,
    pub created_ms: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OutcomeTask {
    pub input: TaskInput,
    pub kind: TaskKind,
    pub title: String,
    pub acceptance: String,
    pub state: TaskState,
    pub paused_from: Option<TaskState>,
    pub revision: u64,
    pub graph_revision: u64,
    pub revisions_used: u32,
    pub nodes_created: u32,
    pub coordinator_calls: u32,
    pub active_ms: u64,
    pub clock_ms: u64,
    pub created_ms: u64,
    pub coordinator_lease: Option<TaskLease>,
    pub nodes: Vec<TaskNode>,
    pub retired_nodes: Vec<TaskNode>,
    pub graph_history: Vec<Vec<TaskNodeInput>>,
    pub coordinator_attempts: Vec<TaskAttempt>,
    pub artifacts: Vec<TaskArtifact>,
    pub deliveries: Vec<TaskDelivery>,
    pub events: Vec<TaskEvent>,
    pub notes: Vec<String>,
    pub question: String,
    pub error: Option<TaskErrorCode>,
    pub reply_to: Option<u64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeReview {
    pub node_id: String,
    pub approved: bool,
    pub reason: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum CoordinatorProposal {
    Clarify {
        question: String,
    },
    Plan {
        title: String,
        acceptance: String,
        nodes: Vec<TaskNodeInput>,
    },
    Revise {
        reason: String,
        nodes: Vec<TaskNodeInput>,
    },
    Check {
        reviews: Vec<NodeReview>,
    },
    Deliver {
        summary: String,
        node_ids: Vec<String>,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoordinatorOutput {
    pub protocol_version: u32,
    pub proposal: CoordinatorProposal,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum TaskAction {
    Supplement {
        text: String,
    },
    Budget {
        budget: TaskBudget,
    },
    Accept,
    Resume,
    Return {
        reason: String,
    },
    Cancel,
    Claim {
        node_id: Option<String>,
        claim_id: String,
    },
    Coordinate {
        claim_id: String,
        output: CoordinatorOutput,
        receipt: Value,
    },
    Submit {
        node_id: String,
        claim_id: String,
        summary: String,
        evidence: Value,
        receipt: Value,
        succeeded: bool,
    },
    Fail {
        node_id: Option<String>,
        claim_id: String,
        code: TaskErrorCode,
    },
    Pause {
        code: TaskErrorCode,
    },
    Tick,
}
impl TaskAction {
    pub fn operator_allowed(&self) -> bool {
        matches!(
            self,
            Self::Resume
                | Self::Supplement { .. }
                | Self::Budget { .. }
                | Self::Accept
                | Self::Return { .. }
                | Self::Cancel
        )
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum TaskCommand {
    Create {
        input: TaskInput,
    },
    Question {
        input: TaskInput,
        people: Vec<String>,
        message: crate::Publish,
    },
    Get {
        id: String,
    },
    List {
        group_id: String,
        after: String,
        active: bool,
        questions: bool,
    },
    Change {
        id: String,
        revision: u64,
        action: TaskAction,
    },
}
