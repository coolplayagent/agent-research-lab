//! Scenarios contribute validation and parameter choices; goals belong to AI-IM.
use crate::*;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Scenario {
    pub id: String,
    pub fields: Vec<ScenarioField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation: Option<ConversationDefaults>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConversationDefaults {
    pub parameters: Value,
    pub max_seconds: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScenarioField {
    pub id: String,
    pub options: Vec<String>,
}
pub trait ScenarioAdapter: Send + Sync {
    fn descriptor(&self) -> Scenario;
    /// Validation is read-only. Durable admission and scheduling follow goal creation.
    fn validate(&self, goal: &crystal::GoalInput) -> Result<()>;
    fn status(&self) -> Value {
        json!({"available":true,"status":contracts::ScenarioStatus::Ready})
    }
}
pub struct Collaboration;
impl ScenarioAdapter for Collaboration {
    fn descriptor(&self) -> Scenario {
        Scenario {
            id: "collaboration".into(),
            fields: vec![],
            conversation: Some(ConversationDefaults {
                parameters: json!({}),
                max_seconds: 300,
            }),
        }
    }
    fn validate(&self, goal: &crystal::GoalInput) -> Result<()> {
        ensure!(
            goal.parameters.is_null() || goal.parameters == json!({}),
            "generic collaboration has no scenario parameters"
        );
        Ok(())
    }
}
