//! Non-code proposals and independent critique use the same agent protocol.
use crate::{ConversationDefaults, Scenario, ScenarioAdapter};
use anyhow::{Result, ensure};
use contracts::{ScenarioCapability, WorkState};
use serde::Deserialize;
use serde_json::json;

pub struct DesignReview;

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Parameters {
    #[serde(default)]
    criteria: Vec<String>,
}

impl ScenarioAdapter for DesignReview {
    fn descriptor(&self) -> Scenario {
        Scenario {
            id: "design-review".into(),
            protocol_version: crystal::SCENARIO_PROTOCOL_VERSION,
            capabilities: vec![ScenarioCapability::Execute],
            assignment_roles: vec![],
            execution_seconds: Some(300),
            public_process: false,
            manual_review: true,
            fields: vec![],
            conversation: Some(ConversationDefaults {
                parameters: json!({}),
                max_seconds: 300,
            }),
        }
    }

    fn validate(&self, goal: &crystal::GoalInput) -> Result<()> {
        let parameters: Parameters = if goal.parameters.is_null() {
            Parameters::default()
        } else {
            serde_json::from_value(goal.parameters.clone())?
        };
        ensure!(
            goal.assignments.len() >= 2,
            "design review requires distinct proposer and reviewer"
        );
        ensure!(
            parameters.criteria.len() <= 16
                && parameters
                    .criteria
                    .iter()
                    .all(|criterion| { !criterion.trim().is_empty() && criterion.len() <= 256 }),
            "design review criteria exceed bounds"
        );
        Ok(())
    }

    fn uses_agent_protocol(&self) -> bool {
        true
    }

    fn assignment_ready(&self, goal: &crystal::Goal, work: &crystal::WorkItem) -> bool {
        goal.work
            .last()
            .is_none_or(|reviewer| reviewer.id != work.id)
            || goal
                .work
                .iter()
                .take(goal.work.len().saturating_sub(1))
                .all(|prior| matches!(prior.state, WorkState::Submitted | WorkState::Failed))
    }
}
