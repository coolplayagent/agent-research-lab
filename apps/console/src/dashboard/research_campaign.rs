//! Explicit host opt-in for recurring, read-only group research. Goals are the durable cursor.
use super::*;
use collaboration_api::ScenarioAdapter;
use collaboration_model::{AssignmentInput, Goal, GoalChange, GoalInput, GoalState, WorkState};
use serde::Deserialize;
use std::collections::BTreeSet;

#[derive(Clone, Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Campaign {
    pub schema_version: u32,
    pub id: String,
    pub group_id: String,
    pub person_ids: [String; 4],
    pub repositories: [String; 4],
    pub objective: String,
    pub acceptance: String,
    pub instructions: [String; 4],
    pub max_seconds: u64,
    pub pause_seconds: u64,
    pub failure_backoff_seconds: u64,
    pub max_rounds: u32,
}
impl Campaign {
    fn validate(&self, c: &Config) -> Result<()> {
        ensure!(self.schema_version == 1, "unsupported campaign schema");
        ensure!(
            self.id.len() <= 32
                && !self.id.is_empty()
                && self
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-'),
            "invalid campaign ID"
        );
        ensure!(
            self.person_ids.iter().collect::<BTreeSet<_>>().len() == 4,
            "campaign requires four distinct Agents"
        );
        ensure!(
            self.repositories.iter().collect::<BTreeSet<_>>().len() == 4
                && self.repositories.iter().all(|r| c.tools.contains_key(r)),
            "campaign requires four configured CLI repositories"
        );
        ensure!(
            c.max_agents >= 4,
            "campaign requires capacity for four Agents"
        );
        ensure!(
            (1..=1_000_000).contains(&self.max_rounds),
            "campaign rounds outside bound"
        );
        ensure!(
            (1..=3600).contains(&self.pause_seconds)
                && (60..=43200).contains(&self.failure_backoff_seconds),
            "invalid campaign backoff"
        );
        self.goal(0).validate()
    }
    fn prefix(&self) -> String {
        format!("campaign-{}-", &storage::digest(self.id.as_bytes())[..32])
    }
    fn policy_digest(&self) -> String {
        storage::digest(&serde_json::to_vec(self).expect("campaign fields are JSON serializable"))
    }
    fn goal(&self, round: u32) -> GoalInput {
        GoalInput {
            id: format!("{}{:07}", self.prefix(), round),
            group_id: self.group_id.clone(),
            title: format!(
                "{} / {} / {}",
                self.id,
                round + 1,
                self.repositories[round as usize % 4]
            ),
            objective: self.objective.clone(),
            acceptance: self.acceptance.clone(),
            scenario: "research".into(),
            parameters: json!({"repository":self.repositories[round as usize % 4],"interaction":contracts::ResearchInteraction::Parallel,"campaign_policy_digest":self.policy_digest()}),
            assignments: self
                .person_ids
                .iter()
                .zip(&self.instructions)
                .map(|(person, instruction)| AssignmentInput {
                    person_id: person.clone(),
                    instruction: instruction.clone(),
                })
                .collect(),
            max_seconds: self.max_seconds,
        }
    }
    fn next_round(&self, goals: &[Goal], now: u64) -> Result<Option<u32>> {
        let mut round = 0;
        for goal in goals {
            ensure!(
                goal.input == self.goal(round),
                "campaign configuration differs from recorded goal"
            );
            if goal.state == GoalState::Cancelled {
                return Ok(None);
            }
            if goal.work.len() != 4
                || !goal.work.iter().all(|w| {
                    matches!(w.state, WorkState::Submitted | WorkState::Failed)
                        || (w.state == WorkState::Running
                            && w.deadline_ms.is_some_and(|deadline| deadline <= now))
                })
            {
                return Ok(None);
            }
            let failed = goal.work.iter().any(|w| {
                w.state == WorkState::Failed
                    || (w.state == WorkState::Running
                        && w.deadline_ms.is_some_and(|deadline| deadline <= now))
            });
            let pause = if failed {
                self.failure_backoff_seconds.max(self.pause_seconds)
            } else {
                self.pause_seconds
            };
            // Deadlines conservatively bound completion when no separate finish timestamp exists.
            let finished_bound = goal
                .work
                .iter()
                .filter_map(|w| w.deadline_ms)
                .max()
                .unwrap_or(
                    goal.created_ms
                        .saturating_add(self.max_seconds.saturating_mul(1000)),
                );
            if now < finished_bound.saturating_add(pause.saturating_mul(1000)) {
                return Ok(None);
            }
            round += 1;
        }
        Ok((round < self.max_rounds).then_some(round))
    }
}

pub(super) async fn admit(hub: &collaboration_client::Client, c: Arc<Config>) -> Result<()> {
    let path = c.state_dir.join("private/research-campaign.json");
    if !path.try_exists()? {
        return Ok(());
    }
    ensure!(
        fs::metadata(&path)?.len() <= 32768,
        "campaign configuration exceeds bound"
    );
    let campaign: Campaign = storage::read(&path)?;
    admit_campaign(hub, c, &campaign, collaboration_model::now_ms()).await
}

async fn admit_campaign(
    hub: &collaboration_client::Client,
    c: Arc<Config>,
    campaign: &Campaign,
    now: u64,
) -> Result<()> {
    campaign.validate(&c)?;
    let mut after = campaign.prefix();
    let mut goals = Vec::new();
    loop {
        let page = hub
            .goals(&campaign.group_id, "research", &after, false)
            .await?;
        let rows: Vec<Goal> = serde_json::from_value(page["goals"].clone())?;
        let mut ended = false;
        for goal in rows {
            if !goal.input.id.starts_with(&campaign.prefix()) {
                ended = true;
                break;
            }
            goals.push(goal);
            ensure!(goals.len() <= 10000, "campaign exceeds storage capacity");
        }
        if ended {
            break;
        }
        let Some(next) = page["next_after"].as_str() else {
            break;
        };
        after = next.into();
    }
    if let Some(round) = campaign.next_round(&goals, now)? {
        let goal = campaign.goal(round);
        super::research_scenario::Research(c).validate(&goal)?;
        // Storage atomically creates the deterministic ID, or validates its existing input.
        hub.change_goal(GoalChange::Create { goal }).await?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "research_campaign_tests.rs"]
mod tests;
