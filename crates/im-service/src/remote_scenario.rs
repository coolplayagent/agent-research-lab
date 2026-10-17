//! A configured scenario is a host adapter, never an agent's native output parser.
use crate::{Scenario, ScenarioAdapter};
use anyhow::{Result, ensure};
use collaboration_model::{EvolutionAction, EvolutionRun, GoalInput};
use contracts::{ActivationState, RoundState, ScenarioCapability};
use service_api::{
    ScenarioReconciliation, ScenarioRegistration, ScenarioRequest, ScenarioValidation,
};
use std::{future::Future, pin::Pin, time::Duration};

pub(crate) struct RemoteScenario {
    pub registration: ScenarioRegistration,
    pub dependencies: Vec<service_api::Service>,
}

impl RemoteScenario {
    async fn ready(&self) -> Result<()> {
        let mut probes = tokio::task::JoinSet::new();
        for service in self.dependencies.clone() {
            probes.spawn(async move {
                ensure!(service.enabled, "scenario dependency disabled");
                service_api::probe(&service.endpoint, service.kind).await?;
                Ok::<_, anyhow::Error>(())
            });
        }
        while let Some(result) = probes.join_next().await {
            result??;
        }
        service_api::probe_scenario(&self.registration).await?;
        Ok(())
    }

    fn capability(&self, capability: ScenarioCapability) -> Result<()> {
        ensure!(
            self.registration.capabilities.contains(&capability),
            "scenario action capability unsupported"
        );
        Ok(())
    }

    async fn reconcile_run(&self, hub: &im_client::Client, run: &EvolutionRun) -> Result<()> {
        self.ready().await?;
        let goal = crate::iteration::current_goal(hub, run).await?;
        let attempts = goal
            .work
            .iter()
            .map(|work| (work.id.clone(), work.attempt))
            .collect();
        ensure!(
            run.rounds
                .last()
                .is_some_and(|round| round.goal_attempts == attempts),
            "scenario attempt fence changed"
        );
        let request_id = format!(
            "scenario-{}-{}-{}",
            &storage::digest(run.input.id.as_bytes())[..24],
            run.revision,
            goal.revision
        );
        let receipt: ScenarioReconciliation = service_protocol::call(
            &self.registration.endpoint,
            &ScenarioRequest::Reconcile {
                request_id: request_id.clone(),
                run: Box::new(run.clone()),
                goal: Box::new(goal.clone()),
            },
            Duration::from_secs(5),
        )
        .await?;
        ensure!(
            receipt.request_id == request_id
                && receipt.run_id == run.input.id
                && receipt.revision == run.revision
                && receipt.goal_revision == goal.revision
                && receipt.goal_attempts == attempts,
            "scenario reconciliation fence mismatch"
        );
        let Some(change) = receipt.change else {
            return Ok(());
        };
        ensure!(
            change.run_id == run.input.id
                && change.revision == run.revision
                && change.request_id == request_id,
            "scenario change identity mismatch"
        );
        match &change.action {
            EvolutionAction::RecordCandidate { .. } => {
                self.capability(ScenarioCapability::Execute)?
            }
            EvolutionAction::Evaluate { .. } | EvolutionAction::Accept => {
                self.capability(ScenarioCapability::Evaluate)?
            }
            EvolutionAction::BeginActivation { .. } => {
                self.capability(ScenarioCapability::Activate)?
            }
            EvolutionAction::Activate { activation } => {
                self.capability(ScenarioCapability::Activate)?;
                if activation.state == ActivationState::RolledBack {
                    self.capability(ScenarioCapability::Rollback)?;
                }
            }
            EvolutionAction::FinishRound { state, .. } => {
                if *state == RoundState::Completed
                    && self
                        .registration
                        .capabilities
                        .contains(&ScenarioCapability::Activate)
                {
                    ensure!(
                        run.rounds
                            .last()
                            .and_then(|round| round.activation.as_ref())
                            .is_some_and(|activation| activation.state == ActivationState::Active),
                        "scenario requires verified activation"
                    );
                }
            }
            _ => anyhow::bail!("scenario cannot modify host iteration policy or lifecycle"),
        }
        let current_goal = crate::iteration::current_goal(hub, run).await?;
        ensure!(
            current_goal.revision == goal.revision
                && current_goal
                    .work
                    .iter()
                    .all(|work| attempts.get(&work.id) == Some(&work.attempt)),
            "scenario result is stale"
        );
        hub.evolution_change(change).await?;
        Ok(())
    }
}

impl ScenarioAdapter for RemoteScenario {
    fn descriptor(&self) -> Scenario {
        Scenario {
            id: self.registration.id.clone(),
            protocol_version: self.registration.protocol_version,
            capabilities: self.registration.capabilities.clone(),
            assignment_roles: vec![],
            execution_seconds: None,
            public_process: false,
            manual_review: false,
            fields: vec![],
            conversation: None,
        }
    }
    fn validate(&self, goal: &GoalInput) -> Result<()> {
        ensure!(
            goal.scenario == self.registration.id,
            "scenario identity mismatch"
        );
        goal.validate()
    }
    fn admission<'a>(
        &'a self,
        goal: &'a GoalInput,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            self.ready().await?;
            let receipt: ScenarioValidation = service_protocol::call(
                &self.registration.endpoint,
                &ScenarioRequest::Validate {
                    goal: Box::new(goal.clone()),
                },
                Duration::from_secs(5),
            )
            .await?;
            ensure!(
                receipt.valid && receipt.goal_sha256 == storage::digest(&serde_json::to_vec(goal)?),
                "scenario validation rejected or mismatched goal"
            );
            Ok(())
        })
    }
    fn uses_agent_protocol(&self) -> bool {
        true
    }
    fn reconcile<'a>(
        &'a self,
        hub: &'a im_client::Client,
        run: &'a EvolutionRun,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(self.reconcile_run(hub, run))
    }
    fn status(&self) -> serde_json::Value {
        serde_json::json!({"available":false,"configured":true,"status":contracts::ScenarioStatus::Unavailable,"health_requires_probe":true})
    }
}
