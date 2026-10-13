//! Scenario-neutral iteration inputs and host-authorized evidence records.
use crate::{AssignmentInput, GoalInput};
use contracts::{ActivationState, CandidateState, EvaluationState, IterationState, RoundState};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

pub const SCENARIO_PROTOCOL_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EvolutionRunInput {
    pub id: String,
    pub group_id: String,
    pub subject_id: String,
    pub scenario_id: String,
    pub title: String,
    pub objective: String,
    pub acceptance: String,
    pub assignments: Vec<AssignmentInput>,
    pub baseline_version_id: String,
    #[serde(default)]
    pub parameters: Value,
    pub max_rounds: u32,
    pub max_seconds: u64,
    pub total_budget_seconds: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EvolutionRun {
    pub input: EvolutionRunInput,
    pub state: IterationState,
    pub revision: u64,
    pub created_ms: u64,
    pub updated_ms: u64,
    pub used_seconds: u64,
    pub baseline_version_id: String,
    pub rounds: Vec<EvolutionRound>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EvolutionRound {
    pub number: u32,
    pub state: RoundState,
    pub goal_id: String,
    pub goal_attempts: BTreeMap<String, u32>,
    pub baseline_version_id: String,
    pub started_ms: u64,
    pub finished_ms: Option<u64>,
    pub feedback: String,
    pub candidate: Option<EvolutionCandidate>,
    pub evaluation: Option<CandidateEvaluation>,
    pub activation: Option<CandidateActivation>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EvolutionCandidate {
    pub version_id: String,
    pub producer_work_id: String,
    pub state: CandidateState,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CandidateEvaluation {
    pub evaluator_work_id: String,
    pub goal_revision: u64,
    pub state: EvaluationState,
    pub summary: String,
    pub evidence: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CandidateActivation {
    pub state: ActivationState,
    pub reference: String,
    pub evidence: Value,
}

/// A durable request key binds the complete command, including its revision.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EvolutionChange {
    pub request_id: String,
    pub run_id: String,
    pub revision: u64,
    pub action: EvolutionAction,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum EvolutionAction {
    Create {
        input: EvolutionRunInput,
    },
    Pause,
    Resume,
    Stop,
    StartRound {
        goal: GoalInput,
    },
    RecordCandidate {
        version_id: String,
        producer_work_id: String,
    },
    Evaluate {
        evaluation: CandidateEvaluation,
    },
    Accept,
    BeginActivation {
        claim_id: String,
        max_seconds: u64,
    },
    Activate {
        activation: CandidateActivation,
    },
    FinishRound {
        state: RoundState,
        feedback: String,
    },
}

impl EvolutionRun {
    /// Shared deterministic admission input; callers cannot silently change policy.
    pub fn next_goal(&self) -> anyhow::Result<GoalInput> {
        let number = self.rounds.len() + 1;
        let context = serde_json::json!({
            "iteration_id": self.input.id,
            "round": number,
            "baseline_version_id": self.baseline_version_id,
            "previous_feedback": self.rounds.last().map(|round| round.feedback.chars().take(512).collect::<String>())
        });
        let goal = GoalInput {
            id: format!(
                "rsi-{}-{number:07}",
                &storage::digest(self.input.id.as_bytes())[..32]
            ),
            group_id: self.input.group_id.clone(),
            title: format!("{} / round {number}", self.input.title),
            objective: format!("{}\n\nIteration context: {context}", self.input.objective),
            acceptance: self.input.acceptance.clone(),
            scenario: self.input.scenario_id.clone(),
            parameters: self.input.parameters.clone(),
            assignments: self.input.assignments.clone(),
            max_seconds: self.input.max_seconds,
        };
        goal.validate()?;
        Ok(goal)
    }
}
