//! Scenarios contribute validation and parameter choices; goals belong to AI-IM.
use crate::*;
use serde::{Deserialize, Serialize};
use std::{future::Future, pin::Pin};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Scenario {
    pub id: String,
    pub protocol_version: u32,
    pub capabilities: Vec<contracts::ScenarioCapability>,
    #[serde(default)]
    pub assignment_roles: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_seconds: Option<u64>,
    #[serde(default)]
    pub public_process: bool,
    #[serde(default = "manual_review_default")]
    pub manual_review: bool,
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
fn manual_review_default() -> bool {
    true
}
pub trait ScenarioAdapter: Send + Sync {
    fn descriptor(&self) -> Scenario;
    /// Validation is read-only. Durable admission and scheduling follow goal creation.
    fn validate(&self, goal: &collaboration_model::GoalInput) -> Result<()>;
    /// Optional versioned remote admission. Configuration alone is not health.
    fn admission<'a>(
        &'a self,
        _goal: &'a collaboration_model::GoalInput,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }
    /// Opt into the replaceable JSON agent executor. Dedicated adapters retain
    /// their own execution admission, preventing duplicate task dispatch.
    fn uses_agent_protocol(&self) -> bool {
        false
    }
    fn assignment_ready(
        &self,
        _goal: &collaboration_model::Goal,
        _work: &collaboration_model::WorkItem,
    ) -> bool {
        true
    }
    /// Reconcile scenario-owned evidence or optional delivery/activation hooks.
    /// Implementations must fence results to current attempts and use durable
    /// request identities before issuing external effects. Worker claims alone
    /// never authorize evaluation, acceptance or activation.
    fn reconcile<'a>(
        &'a self,
        _hub: &'a collaboration_client::Client,
        _run: &'a collaboration_model::EvolutionRun,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }
    fn status(&self) -> Value {
        json!({"available":true,"status":contracts::ScenarioStatus::Ready})
    }
}
pub struct Collaboration;
impl ScenarioAdapter for Collaboration {
    fn descriptor(&self) -> Scenario {
        Scenario {
            id: "collaboration".into(),
            protocol_version: collaboration_model::SCENARIO_PROTOCOL_VERSION,
            capabilities: vec![contracts::ScenarioCapability::Execute],
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
    fn uses_agent_protocol(&self) -> bool {
        true
    }
    fn validate(&self, goal: &collaboration_model::GoalInput) -> Result<()> {
        ensure!(
            goal.parameters.is_null() || goal.parameters == json!({}),
            "generic collaboration has no scenario parameters"
        );
        Ok(())
    }
}

/// Core collaboration and a non-code design scenario require no research setup.
pub fn default_scenarios() -> Arc<Vec<Arc<dyn ScenarioAdapter>>> {
    Arc::new(vec![
        Arc::new(Collaboration),
        Arc::new(crate::design_review::DesignReview),
    ])
}

pub(crate) fn configured(
    builtins: &Arc<Vec<Arc<dyn ScenarioAdapter>>>,
    registry: Option<&service_registry::Registry>,
) -> Result<Arc<Vec<Arc<dyn ScenarioAdapter>>>> {
    let Some(registry) = registry else {
        return Ok(builtins.clone());
    };
    let config = registry.configuration();
    let mut adapters = builtins.as_ref().clone();
    for registration in config.scenarios.iter().filter(|scenario| scenario.enabled) {
        ensure!(
            !adapters
                .iter()
                .any(|adapter| adapter.descriptor().id == registration.id),
            "registered scenario conflicts with an installed adapter"
        );
        adapters.push(Arc::new(crate::remote_scenario::RemoteScenario {
            registration: registration.clone(),
            dependencies: config
                .services
                .iter()
                .filter(|service| registration.dependencies.contains(&service.id))
                .cloned()
                .collect(),
        }));
    }
    Ok(Arc::new(adapters))
}
