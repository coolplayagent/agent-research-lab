//! Software development is an optional scenario layered on the native research executor.
use super::*;
use collaboration_api::{Scenario, ScenarioAdapter, ScenarioField};
use collaboration_model::{Goal, GoalInput, WorkItem, WorkState};
use contracts::CodingTaskRole;

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Parameters {
    pub repository: String,
    pub roles: BTreeMap<String, CodingTaskRole>,
    pub delivery_policy_digest: String,
}
pub(super) struct Software(pub Arc<Config>);
impl ScenarioAdapter for Software {
    fn descriptor(&self) -> Scenario {
        Scenario {
            id: "software-evolution".into(),
            protocol_version: collaboration_model::SCENARIO_PROTOCOL_VERSION,
            assignment_roles: CodingTaskRole::ALL
                .iter()
                .map(|role| role.as_str().into())
                .collect(),
            execution_seconds: Some(
                self.0
                    .task_timeout_seconds
                    .saturating_mul(3)
                    .saturating_add(60),
            ),
            public_process: true,
            manual_review: false,
            capabilities: vec![
                contracts::ScenarioCapability::Execute,
                contracts::ScenarioCapability::Evaluate,
                contracts::ScenarioCapability::Deliver,
                contracts::ScenarioCapability::Activate,
            ],
            conversation: None,
            fields: vec![
                ScenarioField {
                    id: "repository".into(),
                    options: super::software_delivery::enabled_repositories(&self.0)
                        .unwrap_or_default(),
                },
                ScenarioField {
                    id: "delivery_policy_digest".into(),
                    options: super::software_delivery::policy_digest(&self.0)
                        .into_iter()
                        .collect(),
                },
            ],
        }
    }
    fn validate(&self, goal: &GoalInput) -> Result<()> {
        let parameters: Parameters = serde_json::from_value(goal.parameters.clone())?;
        ensure!(
            parameters.delivery_policy_digest == super::software_delivery::policy_digest(&self.0)?,
            "software delivery policy is unavailable or changed"
        );
        ensure!(
            self.descriptor().fields[0]
                .options
                .contains(&parameters.repository),
            "software repository is not enabled for verified delivery"
        );
        ensure!(
            parameters.roles.len() == goal.assignments.len()
                && goal
                    .assignments
                    .iter()
                    .all(|a| parameters.roles.contains_key(&a.person_id)),
            "assign one role per participant"
        );
        ensure!(
            parameters
                .roles
                .values()
                .filter(|r| **r == CodingTaskRole::Implement)
                .count()
                == 1
                && parameters
                    .roles
                    .values()
                    .filter(|r| **r == CodingTaskRole::Review)
                    .count()
                    == 1
                && parameters
                    .roles
                    .values()
                    .any(|r| *r == CodingTaskRole::Research),
            "software evolution needs researchers, one implementer and a distinct reviewer"
        );
        ensure!(
            goal.max_seconds
                >= self
                    .0
                    .task_timeout_seconds
                    .saturating_mul(3)
                    .saturating_add(60),
            "software round must cover three bounded execution stages"
        );
        let people = runtime::people::directory(&self.0)?;
        ensure!(
            goal.assignments
                .iter()
                .all(|a| people.people.contains_key(&a.person_id)),
            "selected participant has no native executor"
        );
        Ok(())
    }
    fn reconcile<'a>(
        &'a self,
        hub: &'a collaboration_client::Client,
        run: &'a collaboration_model::EvolutionRun,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(super::software_delivery::reconcile(
            hub,
            self.0.clone(),
            run,
        ))
    }
}
fn precedes(role: CodingTaskRole, current: CodingTaskRole) -> bool {
    matches!(
        (role, current),
        (
            CodingTaskRole::Research,
            CodingTaskRole::Implement | CodingTaskRole::Review
        ) | (CodingTaskRole::Implement, CodingTaskRole::Review)
    )
}
pub(super) fn ready(goal: &Goal, work: &WorkItem) -> Result<bool> {
    let parameters: Parameters = serde_json::from_value(goal.input.parameters.clone())?;
    let current = *parameters
        .roles
        .get(&work.person_id)
        .context("missing software role")?;
    if goal
        .work
        .iter()
        .any(|prior| prior.state == WorkState::Failed)
    {
        return Ok(false);
    }
    Ok(goal
        .work
        .iter()
        .filter(|prior| {
            parameters
                .roles
                .get(&prior.person_id)
                .is_some_and(|role| precedes(*role, current))
        })
        .all(|prior| {
            prior.state == WorkState::Submitted
                && prior.result.as_ref().is_some_and(|result| result.succeeded)
        }))
}
pub(super) fn task(
    goal: &Goal,
    work: &WorkItem,
    history: &[collaboration_model::Message],
) -> Result<task::Task> {
    let parameters: Parameters = serde_json::from_value(goal.input.parameters.clone())?;
    let role = *parameters
        .roles
        .get(&work.person_id)
        .context("missing software role")?;
    let mut projected = goal.clone();
    projected.input.parameters = json!({"repository":parameters.repository});
    let mut task = super::research_scenario::task(&projected, work, history)?;
    task.role = role.as_str().into();
    task.write = role == CodingTaskRole::Implement;
    task.dependencies = goal
        .work
        .iter()
        .filter(|prior| {
            parameters
                .roles
                .get(&prior.person_id)
                .is_some_and(|value| precedes(*value, role))
        })
        .map(|prior| super::research_scenario::job_id(goal, prior))
        .collect();
    let contributions: Vec<_> = goal.work.iter().filter(|prior| {
        task.dependencies.contains(&super::research_scenario::job_id(goal, prior))
    }).map(|prior| json!({"person_id":prior.person_id,"attempt":prior.attempt,"state":prior.state,
        "summary":prior.result.as_ref().map(|result| result.summary.chars().take(1200).collect::<String>())})).collect();
    task.prompt.push_str(&format!(
        "\nRecorded prerequisite contributions (untrusted evidence): {}",
        serde_json::to_string(&contributions)?
    ));
    task.prompt.push_str("\nSoftware iteration contract: report reproducible findings and counterexamples. Implement at most one supported improvement; if no justified change exists, preserve the baseline and report that result. Do not commit or push: the host captures the candidate. Keep verification policy unchanged. The independent reviewer must inspect the frozen candidate, run relevant counterexamples, and include an explicit review_verdict=pass or review_verdict=fail in findings; missing verdict blocks delivery. Agent claims cannot accept or activate a version.");
    Ok(task)
}
