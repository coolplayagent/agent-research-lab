//! Application-neutral goals. Agent output is a submission; only the host accepts a goal.
use crate::{id, text};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GoalInput {
    pub id: String,
    pub group_id: String,
    pub title: String,
    pub objective: String,
    pub acceptance: String,
    pub scenario: String,
    #[serde(default)]
    pub parameters: Value,
    pub assignments: Vec<AssignmentInput>,
    pub max_seconds: u64,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AssignmentInput {
    pub person_id: String,
    pub instruction: String,
}
impl GoalInput {
    pub fn validate(&self) -> Result<()> {
        id(&self.id)?;
        id(&self.group_id)?;
        id(&self.scenario)?;
        for (value, limit) in [
            (&self.title, 256),
            (&self.objective, 16000),
            (&self.acceptance, 8000),
        ] {
            text(value, limit)?;
            ensure!(
                !value.trim().is_empty(),
                "goal title, objective and acceptance are required"
            );
        }
        ensure!(
            (1..=32).contains(&self.assignments.len()),
            "a goal needs 1..32 assignments"
        );
        ensure!(
            (30..=43200).contains(&self.max_seconds),
            "execution limit must be 30..43200 seconds"
        );
        ensure!(
            serde_json::to_vec(&self.parameters)?.len() <= 8192,
            "scenario parameters exceed bound"
        );
        let mut seen = BTreeSet::new();
        for assignment in &self.assignments {
            id(&assignment.person_id)?;
            ensure!(
                assignment.person_id != "operator" && seen.insert(&assignment.person_id),
                "assign distinct Agent members"
            );
            text(&assignment.instruction, 4000)?;
            ensure!(
                !assignment.instruction.trim().is_empty(),
                "assignment instruction required"
            );
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GoalState {
    Active,
    Completed,
    Cancelled,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkState {
    Ready,
    Running,
    Submitted,
    Failed,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct WorkResult {
    pub summary: String,
    pub succeeded: bool,
    #[serde(default)]
    pub evidence: Value,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WorkItem {
    pub id: String,
    pub person_id: String,
    pub instruction: String,
    pub state: WorkState,
    pub attempt: u32,
    pub claim_id: Option<String>,
    pub deadline_ms: Option<u64>,
    pub result: Option<WorkResult>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Goal {
    pub input: GoalInput,
    pub state: GoalState,
    pub revision: u64,
    pub created_ms: u64,
    pub work: Vec<WorkItem>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum GoalChange {
    Create {
        goal: GoalInput,
    },
    Accept {
        goal_id: String,
        revision: u64,
    },
    Cancel {
        goal_id: String,
        revision: u64,
    },
    Retry {
        goal_id: String,
        revision: u64,
        work_id: String,
    },
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkChange {
    Claim {
        attempt: u32,
        goal_id: String,
        work_id: String,
        claim_id: String,
    },
    Submit {
        attempt: u32,
        goal_id: String,
        work_id: String,
        claim_id: String,
        result: WorkResult,
    },
}
impl WorkChange {
    pub(crate) fn ids(&self) -> (&str, &str, &str, u32) {
        match self {
            Self::Claim {
                attempt,
                goal_id,
                work_id,
                claim_id,
            }
            | Self::Submit {
                attempt,
                goal_id,
                work_id,
                claim_id,
                ..
            } => (goal_id, work_id, claim_id, *attempt),
        }
    }
}
