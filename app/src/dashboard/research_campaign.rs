//! Explicit host opt-in for recurring, read-only group research. Goals are the durable cursor.
use super::*;
use crystal::{AssignmentInput, Goal, GoalChange, GoalInput, GoalState, WorkState};
use im_service::ScenarioAdapter;
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

pub(super) async fn admit(hub: &im_storage::Client, c: Arc<Config>) -> Result<()> {
    let path = c.state_dir.join("private/research-campaign.json");
    if !path.try_exists()? {
        return Ok(());
    }
    ensure!(
        fs::metadata(&path)?.len() <= 32768,
        "campaign configuration exceeds bound"
    );
    let campaign: Campaign = storage::read(&path)?;
    admit_campaign(hub, c, &campaign, crystal::now_ms()).await
}

async fn admit_campaign(
    hub: &im_storage::Client,
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
mod tests {
    use super::*;
    fn campaign() -> Campaign {
        Campaign {
            schema_version: 1,
            id: "fixture".into(),
            group_id: "group".into(),
            person_ids: ["a".into(), "b".into(), "c".into(), "d".into()],
            repositories: ["one".into(), "two".into(), "three".into(), "four".into()],
            objective: "Inspect CLI failure recovery".into(),
            acceptance: "Cite pinned evidence".into(),
            instructions: std::array::from_fn(|_| "Inspect independently".into()),
            max_seconds: 60,
            pause_seconds: 10,
            failure_backoff_seconds: 120,
            max_rounds: 1_000_000,
        }
    }
    fn completed(c: &Campaign) -> Goal {
        Goal {
            input: c.goal(0),
            state: GoalState::Active,
            revision: 5,
            created_ms: 1000,
            work: (0..4)
                .map(|i| crystal::WorkItem {
                    id: format!("work-{i}"),
                    person_id: c.person_ids[i].clone(),
                    instruction: c.instructions[i].clone(),
                    state: WorkState::Submitted,
                    attempt: 1,
                    claim_id: Some("claim".into()),
                    deadline_ms: Some(61000),
                    result: None,
                })
                .collect(),
        }
    }
    #[test]
    fn restart_rebuilds_cursor_and_rotates_four_parallel_members() {
        let c = campaign();
        let goal = completed(&c);
        assert_eq!(c.next_round(&[], 0).unwrap(), Some(0));
        assert_eq!(
            c.next_round(std::slice::from_ref(&goal), 71000).unwrap(),
            Some(1)
        );
        assert_eq!(c.next_round(&[goal], 71000).unwrap(), Some(1));
        assert_eq!(c.goal(1).parameters["repository"], "two");
        assert_eq!(c.goal(1).parameters["interaction"], "parallel");
        assert_eq!(c.goal(1).assignments.len(), 4);
    }
    #[test]
    fn running_failure_and_cancellation_block_early_rounds() {
        let c = campaign();
        let mut goal = completed(&c);
        goal.work[3].state = WorkState::Running;
        assert_eq!(
            c.next_round(std::slice::from_ref(&goal), 60999).unwrap(),
            None
        );
        goal.work[3].state = WorkState::Failed;
        assert_eq!(
            c.next_round(std::slice::from_ref(&goal), 180999).unwrap(),
            None
        );
        assert_eq!(
            c.next_round(std::slice::from_ref(&goal), 181000).unwrap(),
            Some(1)
        );
        goal.state = GoalState::Cancelled;
        assert_eq!(c.next_round(&[goal], 999999).unwrap(), None);
    }
    #[test]
    fn edited_config_or_missing_members_cannot_restart_campaign() {
        let mut c = campaign();
        let goal = completed(&c);
        c.failure_backoff_seconds += 1;
        assert!(c.next_round(&[goal], 999999).is_err());
        let c = campaign();
        let mut goal = completed(&c);
        goal.work.pop();
        assert_eq!(c.next_round(&[goal], 999999).unwrap(), None);
        let mut c = campaign();
        c.max_rounds = 1;
        assert_eq!(c.next_round(&[completed(&c)], 999999).unwrap(), None);
    }
    #[test]
    fn complete_policy_is_prompt_bound_and_invalid_digest_is_rejected() {
        let c = campaign();
        let mut goal = completed(&c);
        let task = super::super::research_scenario::task(&goal, &goal.work[0], &[]).unwrap();
        assert!(task.prompt.contains(&c.policy_digest()));
        for field in ["pause_seconds", "failure_backoff_seconds", "max_rounds"] {
            let mut changed = serde_json::to_value(&c).unwrap();
            changed[field] = json!(changed[field].as_u64().unwrap() + 1);
            let changed: Campaign = serde_json::from_value(changed).unwrap();
            assert!(
                changed
                    .next_round(std::slice::from_ref(&goal), 999999)
                    .is_err()
            );
        }
        goal.input.parameters["campaign_policy_digest"] = json!("invalid");
        assert!(super::super::research_scenario::task(&goal, &goal.work[0], &[]).is_err());
        goal.input
            .parameters
            .as_object_mut()
            .unwrap()
            .remove("campaign_policy_digest");
        assert!(super::super::research_scenario::task(&goal, &goal.work[0], &[]).is_ok());
    }
    #[tokio::test]
    async fn storage_admission_is_durable_idempotent_and_membership_bound() {
        let root = std::env::temp_dir().join(format!(
            "research-campaign-{}-{}",
            std::process::id(),
            crystal::now_ms()
        ));
        fs::create_dir_all(&root).unwrap();
        let c: Config = serde_json::from_value(json!({"schema_version":1,"workspace":root,"state_dir":root.join("state"),"superpod":root,"codex":"unused","workflow":"unused","daily_seconds":600,"max_agents":4,"task_timeout_seconds":60,"require_latest":false,"models":{"research":"fixture"},"tools":{"one":{"binary":"unused","repository":root,"probe":[]},"two":{"binary":"unused","repository":root,"probe":[]},"three":{"binary":"unused","repository":root,"probe":[]},"four":{"binary":"unused","repository":root,"probe":[]}}})).unwrap();
        let mut campaign = campaign();
        campaign.max_seconds = 120;
        for i in 0..4 {
            let person = runtime::people::change(
                &c,
                runtime::people::Change::Create {
                    name: format!("Researcher{i}"),
                    role: "research".into(),
                    kind: runtime::people::Kind::Research,
                    soul: "Inspect evidence".into(),
                    purpose: "CLI research".into(),
                },
            )
            .unwrap();
            campaign.person_ids[i] = person.id;
        }
        let c = Arc::new(c);
        let store = root.join("collaboration");
        let hub = im_storage::Client::from(crystal::Hub::open(&store).unwrap());
        hub.register_operator().await.unwrap();
        hub.register(campaign.person_ids.to_vec()).await.unwrap();
        hub.create_group(crystal::NewGroup {
            id: campaign.group_id.clone(),
            title: "Research".into(),
            topic: "CLI".into(),
            private: false,
            kind: crystal::GroupKind::Conversation,
            members: vec![
                "operator".into(),
                campaign.person_ids[0].clone(),
                campaign.person_ids[1].clone(),
                campaign.person_ids[2].clone(),
            ],
        })
        .await
        .unwrap();
        assert!(admit_campaign(&hub, c.clone(), &campaign, 0).await.is_err());
        assert!(
            hub.goals(&campaign.group_id, "research", "", false)
                .await
                .unwrap()["goals"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        hub.membership(&campaign.group_id, &campaign.person_ids[3], true)
            .await
            .unwrap();
        let (a, b) = tokio::join!(
            admit_campaign(&hub, c.clone(), &campaign, 0),
            admit_campaign(&hub, c.clone(), &campaign, 0)
        );
        a.unwrap();
        b.unwrap();
        let page = hub
            .goals(&campaign.group_id, "research", "", false)
            .await
            .unwrap();
        let goals: Vec<Goal> = serde_json::from_value(page["goals"].clone()).unwrap();
        assert_eq!(goals.len(), 1);
        assert_eq!(goals[0].work.len(), 4);
        assert!(
            goals[0]
                .work
                .iter()
                .all(|w| super::super::research_scenario::turn_ready(&goals[0], w).unwrap())
        );
        let path = c.state_dir.join("private/research-campaign.json");
        storage::write(&path, &campaign).unwrap();
        admit(&hub, c.clone()).await.unwrap();
        let mut edited = campaign.clone();
        edited.objective = "Different objective".into();
        assert!(admit_campaign(&hub, c.clone(), &edited, 0).await.is_err());
        fs::remove_file(&path).unwrap();
        admit(&hub, c.clone()).await.unwrap();
        let mut claimed = goals[0].clone();
        for work in &goals[0].work {
            claimed = hub
                .work_as(
                    &work.person_id,
                    crystal::WorkChange::Claim {
                        attempt: work.attempt,
                        goal_id: goals[0].input.id.clone(),
                        work_id: work.id.clone(),
                        claim_id: format!("claim-{}", work.id),
                    },
                )
                .await
                .unwrap();
        }
        let deadline = claimed
            .work
            .iter()
            .filter_map(|w| w.deadline_ms)
            .max()
            .unwrap();
        admit_campaign(&hub, c.clone(), &campaign, deadline - 1)
            .await
            .unwrap();
        admit_campaign(&hub, c.clone(), &campaign, deadline + 119999)
            .await
            .unwrap();
        assert_eq!(
            hub.goals(&campaign.group_id, "research", "", false)
                .await
                .unwrap()["goals"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        admit_campaign(&hub, c.clone(), &campaign, deadline + 120000)
            .await
            .unwrap();
        assert_eq!(
            hub.goals(&campaign.group_id, "research", "", false)
                .await
                .unwrap()["goals"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        hub.change_goal(GoalChange::Cancel {
            goal_id: campaign.goal(1).id,
            revision: 1,
        })
        .await
        .unwrap();
        drop(hub);
        let hub = im_storage::Client::from(crystal::Hub::open(&store).unwrap());
        admit_campaign(&hub, c, &campaign, u64::MAX).await.unwrap();
        assert_eq!(
            hub.goals(&campaign.group_id, "research", "", false)
                .await
                .unwrap()["goals"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        drop(hub);
        fs::remove_dir_all(root).unwrap();
    }
}
