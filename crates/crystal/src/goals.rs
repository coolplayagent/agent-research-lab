//! Goals share the conversation writer and its transactional authority.
use crate::*;
use rusqlite::{Connection, OptionalExtension, params};
pub(crate) enum GoalCommand {
    Change(GoalChange),
    Work {
        actor: String,
        credential: Option<String>,
        change: WorkChange,
    },
    List {
        group: String,
        actor: Option<String>,
        scenario: String,
        after: String,
        active: bool,
    },
}
impl Hub {
    pub async fn change_goal(&self, change: GoalChange) -> Result<Goal> {
        Ok(serde_json::from_value(
            self.control(Control::Goal(GoalCommand::Change(change)))
                .await?,
        )?)
    }
    pub async fn work(&self, token: &str, change: WorkChange) -> Result<Goal> {
        let actor = self.authenticate(token)?;
        self.work_command(actor, Some(storage::digest(token.as_bytes())), change)
            .await
    }
    /// Trusted scenario adapters only. HTTP workers always use `work` with a grant.
    pub async fn work_as(&self, actor: &str, change: WorkChange) -> Result<Goal> {
        self.work_command(actor.into(), None, change).await
    }
    async fn work_command(
        &self,
        actor: String,
        credential: Option<String>,
        change: WorkChange,
    ) -> Result<Goal> {
        Ok(serde_json::from_value(
            self.control(Control::Goal(GoalCommand::Work {
                actor,
                credential,
                change,
            }))
            .await?,
        )?)
    }
    pub async fn goals(
        &self,
        group: &str,
        scenario: &str,
        after: &str,
        active: bool,
    ) -> Result<Value> {
        self.control(Control::Goal(GoalCommand::List {
            group: group.into(),
            actor: None,
            scenario: scenario.into(),
            after: after.into(),
            active,
        }))
        .await
    }
    pub async fn assigned_goals(&self, token: &str, after: &str) -> Result<Value> {
        let actor = self.authenticate(token)?;
        let result = self
            .control(Control::Goal(GoalCommand::List {
                group: String::new(),
                actor: Some(actor),
                scenario: String::new(),
                after: after.into(),
                active: true,
            }))
            .await?;
        self.authenticate(token)?;
        Ok(result)
    }
}
pub(crate) fn read(connection: &Connection, id: &str) -> Result<Goal> {
    let value: String =
        connection.query_row("SELECT value FROM goals WHERE id=?1", [id], |r| r.get(0))?;
    Ok(serde_json::from_str(&value)?)
}
pub(crate) fn save(connection: &rusqlite::Transaction<'_>, goal: &Goal, note: &str) -> Result<()> {
    connection.execute(
        "UPDATE goals SET value=?2,active=?3 WHERE id=?1",
        params![
            goal.input.id,
            serde_json::to_string(goal)?,
            goal.state == GoalState::Active
        ],
    )?;
    let input = Publish {
        group_id: goal.input.group_id.clone(),
        request_id: format!(
            "goal-{}-{}",
            &storage::digest(goal.input.id.as_bytes())[..20],
            goal.revision
        ),
        text: format!(
            "目标「{}」：{}",
            goal.input.title,
            note.chars().take(800).collect::<String>()
        ),
        reply_to: None,
    };
    let count: u64 = connection.query_row("SELECT COUNT(*) FROM messages", [], |r| r.get(0))?;
    store::append(
        connection,
        "operator",
        &input,
        &mut MAX_MESSAGES.saturating_sub(count),
        true,
    )?;
    Ok(())
}
pub(crate) fn member(connection: &Connection, group: &str, actor: &str) -> Result<()> {
    ensure!(
        connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM members WHERE group_id=?1 AND actor_id=?2)",
            params![group, actor],
            |r| r.get::<_, bool>(0)
        )?,
        "assignee is not a group member"
    );
    Ok(())
}
pub(crate) fn available(connection: &Connection, group: &str) -> Result<()> {
    let (archived, kind): (bool, String) = connection.query_row(
        "SELECT archived,kind FROM groups WHERE id=?1",
        [group],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    ensure!(
        !archived && kind != "board",
        "goals require an active conversation"
    );
    member(connection, group, "operator")
}
impl Store {
    pub(crate) fn goal_command(&mut self, command: GoalCommand) -> Result<Value> {
        match command {
            GoalCommand::Change(change) => self.goal_change(change),
            GoalCommand::Work {
                actor,
                credential,
                change,
            } => self.goal_work(&actor, credential.as_deref(), change),
            GoalCommand::List {
                group,
                actor,
                scenario,
                after,
                active,
            } => {
                ensure!(
                    group.len() <= 100 && scenario.len() <= 100 && after.len() <= 100,
                    "invalid goal cursor"
                );
                let mut statement = self.connection.prepare("SELECT value FROM goals g WHERE (?1='' OR group_id=?1) AND (?2='' OR scenario=?2) AND id>?3 AND (?4=0 OR active=1) AND (?5 IS NULL OR (EXISTS(SELECT 1 FROM goal_assignees a WHERE a.goal_id=g.id AND a.actor_id=?5) AND EXISTS(SELECT 1 FROM members m WHERE m.group_id=g.group_id AND m.actor_id=?5))) ORDER BY id LIMIT 51")?;
                let rows = statement
                    .query_map(params![group, scenario, after, active, actor], |r| {
                        r.get::<_, String>(0)
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                let mut goals = rows
                    .into_iter()
                    .map(|s| serde_json::from_str::<Goal>(&s))
                    .collect::<serde_json::Result<Vec<_>>>()?;
                let next = (goals.len() > 50).then(|| goals[49].input.id.clone());
                goals.truncate(50);
                Ok(json!({"goals":goals,"next_after":next}))
            }
        }
    }
    fn goal_change(&mut self, change: GoalChange) -> Result<Value> {
        let tx = self.connection.transaction()?;
        if let GoalChange::Create { goal: input } = change {
            input.validate()?;
            let prior = tx
                .query_row("SELECT value FROM goals WHERE id=?1", [&input.id], |r| {
                    r.get::<_, String>(0)
                })
                .optional()?;
            if let Some(prior) = prior {
                let prior: Goal = serde_json::from_str(&prior)?;
                ensure!(
                    prior.input == input,
                    "goal ID already binds different inputs"
                );
                return Ok(json!(prior));
            }
            available(&tx, &input.group_id)?;
            ensure!(
                tx.query_row("SELECT COUNT(*) FROM goals", [], |r| r.get::<_, usize>(0))? < 10000,
                "goal capacity exhausted"
            );
            for assignment in &input.assignments {
                member(&tx, &input.group_id, &assignment.person_id)?;
            }
            let work = input
                .assignments
                .iter()
                .enumerate()
                .map(|(i, a)| WorkItem {
                    id: format!("work-{i}"),
                    person_id: a.person_id.clone(),
                    instruction: a.instruction.clone(),
                    state: WorkState::Ready,
                    attempt: 1,
                    claim_id: None,
                    deadline_ms: None,
                    result: None,
                })
                .collect();
            let goal = Goal {
                input,
                state: GoalState::Active,
                revision: 1,
                created_ms: now_ms(),
                work,
            };
            tx.execute(
                "INSERT INTO goals(id,group_id,scenario,active,value) VALUES(?1,?2,?3,1,'')",
                params![goal.input.id, goal.input.group_id, goal.input.scenario],
            )?;
            for work in &goal.work {
                tx.execute(
                    "INSERT INTO goal_assignees(goal_id,actor_id) VALUES(?1,?2)",
                    params![goal.input.id, work.person_id],
                )?;
            }
            save(&tx, &goal, "已创建并分配任务，等待 Agent 领取")?;
            tx.commit()?;
            return Ok(json!(goal));
        }
        let (id, revision) = match &change {
            GoalChange::Accept { goal_id, revision }
            | GoalChange::Cancel { goal_id, revision }
            | GoalChange::Retry {
                goal_id, revision, ..
            } => (goal_id, revision),
            _ => unreachable!(),
        };
        let mut goal = read(&tx, id)?;
        ensure!(
            goal.revision == *revision && goal.state == GoalState::Active,
            "goal changed; refresh before editing"
        );
        available(&tx, &goal.input.group_id)?;
        let note = match change {
            GoalChange::Accept { .. } => {
                ensure!(
                    goal.work.iter().all(|w| w.state == WorkState::Submitted),
                    "every assignment needs a successful submission before acceptance"
                );
                goal.state = GoalState::Completed;
                "主持人已验收目标".to_owned()
            }
            GoalChange::Cancel { .. } => {
                ensure!(
                    goal.work.iter().all(|w| w.state != WorkState::Running
                        || w.deadline_ms.is_some_and(|d| d <= now_ms())),
                    "wait for running assignments to finish or reach their execution limit before cancelling"
                );
                goal.state = GoalState::Cancelled;
                "主持人已取消目标".to_owned()
            }
            GoalChange::Retry { work_id, .. } => {
                let work = goal
                    .work
                    .iter_mut()
                    .find(|w| w.id == work_id)
                    .context("unknown assignment")?;
                ensure!(
                    work.state == WorkState::Failed
                        || work.state == WorkState::Submitted
                        || (work.state == WorkState::Running
                            && work.deadline_ms.is_some_and(|d| d <= now_ms())),
                    "assignment cannot be retried while active"
                );
                member(&tx, &goal.input.group_id, &work.person_id)?;
                ensure!(work.attempt < 10, "assignment retry limit reached");
                work.attempt += 1;
                work.state = WorkState::Ready;
                work.claim_id = None;
                work.deadline_ms = None;
                work.result = None;
                format!(
                    "{} 的任务已退回重做（第 {} 次）",
                    work.person_id, work.attempt
                )
            }
            _ => unreachable!(),
        };
        goal.revision += 1;
        save(&tx, &goal, &note)?;
        tx.commit()?;
        Ok(json!(goal))
    }
}
