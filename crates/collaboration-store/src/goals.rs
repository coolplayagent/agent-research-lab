//! Goals share the conversation writer and its transactional authority.
use crate::*;
use contracts::GoalEventKind;
use rusqlite::{Connection, OptionalExtension, params};
pub(crate) enum GoalCommand {
    Get {
        id: String,
    },
    Discuss {
        input: GoalInput,
        message: Publish,
    },
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
    Current {
        group: String,
    },
}
impl Hub {
    pub async fn goal(&self, id: &str) -> Result<Goal> {
        Ok(serde_json::from_value(
            self.control(Control::Goal(GoalCommand::Get { id: id.into() }))
                .await?,
        )?)
    }
    pub async fn discuss(&self, input: GoalInput, message: Publish) -> Result<Value> {
        self.control(Control::Goal(GoalCommand::Discuss { input, message }))
            .await
    }

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
    pub async fn current_goal(&self, group: &str) -> Result<Value> {
        self.control(Control::Goal(GoalCommand::Current {
            group: group.into(),
        }))
        .await
    }
}
pub(crate) fn read(connection: &Connection, id: &str) -> Result<Goal> {
    let value: String =
        connection.query_row("SELECT value FROM goals WHERE id=?1", [id], |r| r.get(0))?;
    Ok(serde_json::from_str(&value)?)
}
pub(crate) fn save(
    connection: &rusqlite::Transaction<'_>,
    goal: &Goal,
    kind: GoalEventKind,
    work: Option<&WorkItem>,
) -> Result<()> {
    connection.execute(
        "UPDATE goals SET value=?2,active=?3 WHERE id=?1",
        params![
            goal.input.id,
            serde_json::to_string(goal)?,
            goal.state == GoalState::Active
        ],
    )?;
    let contribution = work.filter(|w| {
        kind == GoalEventKind::Submitted
            && w.result
                .as_ref()
                .is_some_and(|result| result.code.is_none())
    });
    let reply_to = if contribution.is_some() {
        connection.query_row("SELECT COALESCE(reply_to,sequence) FROM messages WHERE group_id=?1 AND json_extract(event,'$.goal_id')=?2 AND json_extract(event,'$.kind')=?3 ORDER BY sequence LIMIT 1",params![goal.input.group_id,goal.input.id,GoalEventKind::Created.as_str()],|row| row.get(0)).optional()?
    } else {
        None
    };
    let input = Publish {
        group_id: goal.input.group_id.clone(),
        request_id: format!(
            "goal-{}-{}",
            &storage::digest(goal.input.id.as_bytes())[..20],
            goal.revision
        ),
        text: contribution
            .map(|w| {
                w.result
                    .as_ref()
                    .unwrap()
                    .summary
                    .chars()
                    .take(1000)
                    .collect()
            })
            .unwrap_or_else(|| kind.as_str().into()),
        reply_to,
    };
    let count: u64 = connection.query_row("SELECT COUNT(*) FROM messages", [], |r| r.get(0))?;
    let receipt = store::append(
        connection,
        contribution.map_or("operator", |work| work.person_id.as_str()),
        &input,
        &mut MAX_MESSAGES.saturating_sub(count),
        true,
    )?;
    ensure!(!receipt.duplicate, "audit request ID collision");
    let event = GoalEvent {
        kind,
        goal_id: goal.input.id.clone(),
        title: goal.input.title.clone(),
        person_id: work.map(|w| w.person_id.clone()),
        work_id: work.map(|w| w.id.clone()),
        attempt: work.map(|w| w.attempt),
    };
    connection.execute(
        "UPDATE messages SET event=?2 WHERE sequence=?1",
        params![receipt.message.sequence, serde_json::to_string(&event)?],
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
    fn goal_projection(&self, goals: &[Goal], next: Option<String>) -> Result<Value> {
        let mut projection = serde_json::to_value(goals)?;
        for goal in projection.as_array_mut().unwrap() {
            let iteration: Option<String> = self
                .connection
                .query_row(
                    "SELECT run_id FROM evolution_goal_bindings WHERE goal_id=?1",
                    [goal["input"]["id"].as_str().unwrap()],
                    |row| row.get(0),
                )
                .optional()?;
            if let Some(iteration) = iteration {
                goal["iteration_id"] = json!(iteration);
            }
        }
        Ok(json!({"goals":projection,"next_after":next}))
    }

    fn current_goal(&self, group: &str) -> Result<Value> {
        id(group)?;
        member(&self.connection, group, "operator")?;
        let now = now_ms();
        let value: Option<String> = self
            .connection
            .query_row(
                "SELECT value FROM goals g WHERE group_id=?1 ORDER BY
             CASE WHEN json_extract(value,'$.state')=?2 THEN
               CASE WHEN EXISTS(SELECT 1 FROM json_each(g.value,'$.work') w
                   WHERE json_extract(w.value,'$.state')=?3
                   AND json_type(w.value,'$.deadline_ms') IN ('integer','real')
                   AND json_extract(w.value,'$.deadline_ms')>?5
                   AND json_extract(w.value,'$.deadline_ms')<1.7976931348623157e308) THEN 3
                 WHEN EXISTS(SELECT 1 FROM json_each(g.value,'$.work') w
                   WHERE json_extract(w.value,'$.state')=?4) THEN 2
                 ELSE 1 END
               ELSE 0 END DESC,
             json_extract(value,'$.created_ms') DESC, id ASC LIMIT 1",
                params![
                    group,
                    GoalState::Active.as_str(),
                    WorkState::Running.as_str(),
                    WorkState::Ready.as_str(),
                    now
                ],
                |row| row.get(0),
            )
            .optional()?;
        let goals = value
            .map(|value| serde_json::from_str::<Goal>(&value))
            .transpose()?
            .into_iter()
            .collect::<Vec<_>>();
        self.goal_projection(&goals, None)
    }

    pub(crate) fn goal_command(&mut self, command: GoalCommand) -> Result<Value> {
        match command {
            GoalCommand::Get { id: goal_id } => {
                id(&goal_id)?;
                Ok(json!(read(&self.connection, &goal_id)?))
            }
            GoalCommand::Current { group } => self.current_goal(&group),
            GoalCommand::Change(change) => self.goal_change(change),
            GoalCommand::Discuss { input, message } => {
                ensure!(
                    input.group_id == message.group_id && input.objective == message.text,
                    "discussion must bind its original message"
                );
                let expected = format!(
                    "mention-{}",
                    storage::digest(
                        format!("{}:{}", message.group_id, message.request_id).as_bytes()
                    )
                );
                ensure!(
                    input.id == expected,
                    "discussion ID must bind its message request"
                );
                let tx = self.connection.transaction()?;
                let count: u64 = tx.query_row("SELECT COUNT(*) FROM messages", [], |r| r.get(0))?;
                let receipt = store::append(
                    &tx,
                    "operator",
                    &message,
                    &mut MAX_MESSAGES.saturating_sub(count),
                    true,
                )?;
                let goal = create_goal(&tx, input)?;
                tx.execute("UPDATE messages SET reply_to=?1 WHERE group_id=?2 AND json_extract(event,'$.goal_id')=?3 AND json_extract(event,'$.kind')=?4", params![receipt.message.sequence, goal.input.group_id, goal.input.id, GoalEventKind::Created.as_str()])?;
                tx.commit()?;
                Ok(json!({"message":receipt.message,"goal":goal,"duplicate":receipt.duplicate}))
            }
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
                self.goal_projection(&goals, next)
            }
        }
    }
    fn goal_change(&mut self, change: GoalChange) -> Result<Value> {
        let tx = self.connection.transaction()?;
        if let GoalChange::Create { goal: input } = change {
            let goal = create_goal(&tx, input)?;
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
        ensure!(
            !tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM evolution_goal_bindings WHERE goal_id=?1)",
                [&goal.input.id],
                |row| row.get::<_, bool>(0)
            )?,
            "iteration-owned goals require iteration operations; retry in a new round"
        );
        let mut changed_work = None;
        let kind = match change {
            GoalChange::Accept { .. } => {
                ensure!(
                    goal.work.iter().all(|w| w.state == WorkState::Submitted),
                    "every assignment needs a successful submission before acceptance"
                );
                goal.state = GoalState::Completed;
                GoalEventKind::Accepted
            }
            GoalChange::Cancel { .. } => {
                ensure!(
                    goal.work.iter().all(|w| w.state != WorkState::Running
                        || w.deadline_ms.is_some_and(|d| d <= now_ms())),
                    "wait for running assignments to finish or reach their execution limit before cancelling"
                );
                goal.state = GoalState::Cancelled;
                GoalEventKind::Cancelled
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
                changed_work = Some(work.clone());
                GoalEventKind::Retried
            }
            _ => unreachable!(),
        };
        goal.revision += 1;
        save(&tx, &goal, kind, changed_work.as_ref())?;
        tx.commit()?;
        Ok(json!(goal))
    }
}

pub(crate) fn create_goal(tx: &rusqlite::Transaction<'_>, input: GoalInput) -> Result<Goal> {
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
        return Ok(prior);
    }
    available(tx, &input.group_id)?;
    ensure!(
        tx.query_row("SELECT COUNT(*) FROM goals", [], |r| r.get::<_, usize>(0))? < 10000,
        "goal capacity exhausted"
    );
    for assignment in &input.assignments {
        member(tx, &input.group_id, &assignment.person_id)?;
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
    save(tx, &goal, GoalEventKind::Created, None)?;

    Ok(goal)
}
