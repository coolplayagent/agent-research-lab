//! Tasks are changed only by the collaboration storage writer.
use crate::*;
use rusqlite::{Connection, OptionalExtension, params};
impl Hub {
    pub async fn tasks(&self, command: TaskCommand) -> Result<Value> {
        self.control(Control::Task(Box::new(command))).await
    }
}
pub(crate) fn read(db: &Connection, id: &str) -> Result<OutcomeTask> {
    let raw: Option<String> = db
        .query_row("SELECT value FROM outcome_tasks WHERE id=?1", [id], |r| {
            r.get(0)
        })
        .optional()?;
    Ok(serde_json::from_str(
        &raw.context(contracts::TaskErrorCode::NotFound)?,
    )?)
}
pub(crate) fn member_active(db: &Connection, group: &str, actor: &str) -> Result<bool> {
    let mut query = db.prepare("SELECT value FROM outcome_tasks WHERE group_id=?1 AND active=1")?;
    for raw in query.query_map([group], |r| r.get::<_, String>(0))? {
        let task: OutcomeTask = serde_json::from_str(&raw?)?;
        if actor == "operator" || task.nodes.iter().any(|n| n.input.person_id == actor) {
            return Ok(true);
        }
    }
    Ok(false)
}
fn members(db: &Connection, group: &str) -> Result<BTreeSet<String>> {
    Ok(db.prepare("SELECT m.actor_id FROM members m JOIN actors a ON a.id=m.actor_id WHERE m.group_id=?1 AND a.archived=0")?.query_map([group],|r|r.get(0))?.collect::<rusqlite::Result<_>>()?)
}
impl Store {
    pub(crate) fn task_command(&mut self, command: TaskCommand) -> Result<Value> {
        let tx = self.connection.transaction()?;
        let now = now_ms();
        let result = match command {
            TaskCommand::Get { id } => {
                let task = read(&tx, &id)?;
                goals::member(&tx, &task.input.group_id, "operator")?;
                json!(task)
            }
            TaskCommand::List {
                group_id,
                after,
                active,
                questions,
            } => {
                ensure!(
                    group_id.len() <= 100 && after.len() <= 100,
                    "{}",
                    contracts::TaskErrorCode::Invalid
                );
                let mut q=tx.prepare("SELECT rowid,value FROM outcome_tasks WHERE (?1='' OR group_id=?1) AND (?2='' OR rowid<CAST(?2 AS INTEGER)) AND (?3=0 OR active=1) AND (?4=1 OR kind=?5) AND EXISTS(SELECT 1 FROM members WHERE group_id=outcome_tasks.group_id AND actor_id='operator') ORDER BY rowid DESC LIMIT 51")?;
                let mut rows = vec![];
                let mut bytes = 0usize;
                let mut more = false;
                let mut cursor = String::new();
                for row in q.query_map(
                    params![
                        group_id,
                        after,
                        active,
                        questions,
                        contracts::TaskKind::Outcome.as_str()
                    ],
                    |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)),
                )? {
                    let (sequence, raw) = row?;
                    if rows.len() >= 50 || (!rows.is_empty() && bytes + raw.len() > 6 * 1024 * 1024)
                    {
                        more = true;
                        break;
                    }
                    cursor = sequence.to_string();
                    bytes += raw.len();
                    rows.push(serde_json::from_str::<OutcomeTask>(&raw)?);
                }
                let next = if more { Some(cursor) } else { None };
                json!({"tasks":rows,"next_after":next})
            }
            TaskCommand::Create { input } => {
                goals::available(&tx, &input.group_id)?;
                let existing: Option<String> = tx
                    .query_row(
                        "SELECT value FROM outcome_tasks WHERE id=?1",
                        [&input.id],
                        |r| r.get(0),
                    )
                    .optional()?;
                if let Some(raw) = existing {
                    let task: OutcomeTask = serde_json::from_str(&raw)?;
                    ensure!(
                        task.input == input && task.kind == contracts::TaskKind::Outcome,
                        "{}",
                        contracts::TaskErrorCode::Conflict
                    );
                    json!(task)
                } else {
                    ensure!(
                        tx.query_row("SELECT COUNT(*) FROM outcome_tasks", [], |r| r
                            .get::<_, u64>(0))?
                            < 10000,
                        "{}",
                        contracts::TaskErrorCode::Budget
                    );
                    let mut task = OutcomeTask::new(input, now)?;
                    task.record(contracts::TaskEventKind::Created, None, String::new(), now);
                    insert(&tx, &task)?;
                    json!(task)
                }
            }
            TaskCommand::Question {
                input,
                people,
                message,
            } => {
                ensure!(
                    input.group_id == message.group_id && input.intent == message.text,
                    "{}",
                    contracts::TaskErrorCode::Invalid
                );
                let expected = format!(
                    "question-{}",
                    storage::digest(
                        format!("{}:{}", message.group_id, message.request_id).as_bytes()
                    )
                );
                ensure!(
                    input.id == expected,
                    "{}",
                    contracts::TaskErrorCode::Invalid
                );
                goals::available(&tx, &input.group_id)?;
                let prior: Option<String> = tx
                    .query_row(
                        "SELECT value FROM outcome_tasks WHERE id=?1",
                        [&input.id],
                        |r| r.get(0),
                    )
                    .optional()?;
                let count =
                    tx.query_row("SELECT COUNT(*) FROM messages", [], |r| r.get::<_, u64>(0))?;
                let receipt = store::append(
                    &tx,
                    "operator",
                    &message,
                    &mut MAX_MESSAGES.saturating_sub(count),
                    true,
                )?;
                if let Some(raw) = prior {
                    let task: OutcomeTask = serde_json::from_str(&raw)?;
                    ensure!(
                        task.input == input
                            && task
                                .nodes
                                .iter()
                                .map(|n| &n.input.person_id)
                                .eq(people.iter()),
                        "{}",
                        contracts::TaskErrorCode::Conflict
                    );
                    json!({"task":task,"message":receipt.message})
                } else {
                    ensure!(
                        !people.is_empty()
                            && people.len() <= 32
                            && people.iter().collect::<BTreeSet<_>>().len() == people.len(),
                        "{}",
                        contracts::TaskErrorCode::Invalid
                    );
                    let mut task = OutcomeTask::new(input, now)?;
                    task.kind = contracts::TaskKind::Question;
                    task.reply_to = Some(receipt.message.sequence);
                    let nodes=people.into_iter().enumerate().map(|(i,person_id)|TaskNodeInput{id:format!("answer-{i}"),person_id,title:"Answer".into(),instruction:"Answer the user's question directly in their language. Do not wait for other members or claim host acceptance.".into(),deliverable:"A substantive answer with limitations".into(),acceptance:"Answer this individual request".into(),dependencies:vec![]}).collect();
                    task.replace_graph(nodes, &members(&tx, &task.input.group_id)?, now)?;
                    insert(&tx, &task)?;
                    json!({"task":task,"message":receipt.message})
                }
            }
            TaskCommand::Change {
                id,
                revision,
                action,
            } => {
                let mut task = read(&tx, &id)?;
                ensure!(
                    task.revision == revision,
                    "{}",
                    contracts::TaskErrorCode::Conflict
                );
                if !matches!(action, TaskAction::Cancel | TaskAction::Tick) {
                    goals::available(&tx, &task.input.group_id)?;
                }
                let submission = match &action {
                    TaskAction::Submit {
                        node_id, summary, ..
                    } if task.kind == contracts::TaskKind::Question => {
                        Some((node_id.clone(), summary.clone()))
                    }
                    _ => None,
                };
                task.transition(action, &members(&tx, &task.input.group_id)?, now)?;
                if let Some((node, summary)) = submission {
                    let person = &task
                        .nodes
                        .iter()
                        .find(|n| n.input.id == node)
                        .unwrap()
                        .input
                        .person_id;
                    let mut body = summary.as_str();
                    let mut part = 0;
                    while !body.is_empty() {
                        let mut end = body.len().min(MAX_TEXT);
                        while !body.is_char_boundary(end) {
                            end -= 1;
                        }
                        let count = tx.query_row("SELECT COUNT(*) FROM messages", [], |r| {
                            r.get::<_, u64>(0)
                        })?;
                        store::append(
                            &tx,
                            person,
                            &Publish {
                                group_id: task.input.group_id.clone(),
                                request_id: format!(
                                    "answer-{}-{}-{}-{part}",
                                    &storage::digest(id.as_bytes())[..20],
                                    task.nodes
                                        .iter()
                                        .find(|n| n.input.id == node)
                                        .unwrap()
                                        .attempt,
                                    node
                                ),
                                text: body[..end].into(),
                                reply_to: task.reply_to,
                            },
                            &mut MAX_MESSAGES.saturating_sub(count),
                            true,
                        )?;
                        body = &body[end..];
                        part += 1;
                    }
                }
                tx.execute(
                    "UPDATE outcome_tasks SET active=?2,value=?3 WHERE id=?1",
                    params![id, task.active(), serde_json::to_string(&task)?],
                )?;
                crate::outcomes::record_task_artifacts(&tx, &task)?;
                json!(task)
            }
        };
        tx.commit()?;
        Ok(result)
    }
}
fn insert(db: &Connection, task: &OutcomeTask) -> Result<()> {
    ensure!(
        db.query_row("SELECT COUNT(*) FROM outcome_tasks", [], |r| r
            .get::<_, u64>(0))?
            < 10000,
        "{}",
        contracts::TaskErrorCode::Budget
    );
    ensure!(
        !db.query_row(
            "SELECT EXISTS(SELECT 1 FROM goals WHERE id=?1)",
            [&task.input.id],
            |r| r.get::<_, bool>(0)
        )?,
        "{}",
        contracts::TaskErrorCode::Conflict
    );
    db.execute(
        "INSERT INTO outcome_tasks(id,group_id,kind,active,value) VALUES(?1,?2,?3,?4,?5)",
        params![
            task.input.id,
            task.input.group_id,
            task.kind.as_str(),
            task.active(),
            serde_json::to_string(task)?
        ],
    )?;
    Ok(())
}
#[cfg(test)]
#[path = "tasks_tests.rs"]
mod tests;
