//! Atomic human-message intake, task association and bounded chat projections.
use crate::*;
use rusqlite::{Connection, OptionalExtension, Transaction, params};

pub(crate) fn intake(tx: &Transaction<'_>, message: Publish, now: u64) -> Result<Value> {
    let mut remaining =
        MAX_MESSAGES.saturating_sub(
            tx.query_row("SELECT COUNT(*) FROM messages", [], |r| r.get::<_, u64>(0))?,
        );
    let receipt = store::append(tx, "operator", &message, &mut remaining, true)?;
    let group: (String, bool) = tx.query_row(
        "SELECT kind,private FROM groups WHERE id=?1",
        [&message.group_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    if GroupKind::parse(&group.0)? != GroupKind::Conversation || group.1 {
        return Ok(json!(receipt));
    }
    let id = format!(
        "message-{}",
        storage::digest(format!("{}:{}", message.group_id, message.request_id).as_bytes())
    );
    if receipt.duplicate {
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM outcome_tasks WHERE id=?1)",
            [&id],
            |r| r.get(0),
        )?;
        if !exists {
            return Ok(json!(receipt));
        }
        return Ok(
            json!({"message":receipt.message,"task":tasks::read(tx,&id)?,"duplicate":true,"durable":true}),
        );
    }
    let mut task = OutcomeTask::new(
        TaskInput {
            id,
            group_id: message.group_id.clone(),
            intent: message.text,
            budget: TaskBudget::default(),
        },
        now,
    )?;
    task.kind = contracts::TaskKind::Message;
    task.message_processing = Some(contracts::MessageProcessingState::Queued);
    task.reply_to = Some(receipt.message.sequence);
    task.message_sequences.push(receipt.message.sequence);
    if let Some(reply) = message.reply_to {
        // Only host-recorded associations in this conversation can select an intent.
        let raw: Option<String> = tx.query_row("SELECT value FROM outcome_tasks WHERE group_id=?1 AND (json_extract(value,'$.reply_to')=?2 OR EXISTS(SELECT 1 FROM json_each(outcome_tasks.value,'$.message_sequences') AS link WHERE link.value=?2)) ORDER BY rowid DESC LIMIT 1", params![message.group_id,reply], |r| r.get(0)).optional()?;
        if let Some(raw) = raw {
            let prior: OutcomeTask = serde_json::from_str(&raw)?;
            task.reply_task_id = Some(prior.linked_task_id.unwrap_or(prior.input.id));
        }
    }
    task.record(contracts::TaskEventKind::Created, None, String::new(), now);
    project(tx, &mut task, 0)?;
    tasks::insert(tx, &task)?;
    Ok(json!({"message":receipt.message,"task":task,"duplicate":false,"durable":true}))
}

pub(crate) fn validate_route(
    tx: &Transaction<'_>,
    source: &OutcomeTask,
    action: &TaskAction,
) -> Result<()> {
    if source.kind == contracts::TaskKind::Message
        && matches!(
            action,
            TaskAction::Coordinate {
                output: CoordinatorOutput {
                    proposal: CoordinatorProposal::Plan { .. },
                    ..
                },
                ..
            }
        )
        && let Some(id) = &source.reply_task_id
    {
        let target = tasks::read(tx, id)?;
        ensure!(
            target.input.group_id == source.input.group_id,
            "{}",
            contracts::TaskErrorCode::Forbidden
        );
        ensure!(
            matches!(
                target.state,
                contracts::TaskState::Completed | contracts::TaskState::Cancelled
            ),
            "{}",
            contracts::TaskErrorCode::Conflict
        );
    }
    Ok(())
}

pub(crate) fn attach(
    tx: &Transaction<'_>,
    source: &OutcomeTask,
    action: &TaskAction,
    now: u64,
) -> Result<()> {
    let TaskAction::Coordinate {
        output:
            CoordinatorOutput {
                proposal:
                    CoordinatorProposal::Attach {
                        task_id,
                        revision,
                        text,
                    },
                ..
            },
        ..
    } = action
    else {
        return Ok(());
    };
    let mut target = tasks::read(tx, task_id)?;
    ensure!(
        target.input.group_id == source.input.group_id
            && target.input.id != source.input.id
            && target.kind != contracts::TaskKind::Question,
        "{}",
        contracts::TaskErrorCode::Forbidden
    );
    // Execution progress does not invalidate a routing decision. A concurrent human
    // edit does: retry routing against the updated intent instead of overwriting it.
    ensure!(
        target.last_intent_revision <= *revision && *revision <= target.revision,
        "{}",
        contracts::TaskErrorCode::Conflict
    );
    if let Some(reply) = &source.reply_task_id {
        ensure!(reply == task_id, "{}", contracts::TaskErrorCode::Conflict);
    }
    let start = target.events.len();
    let members = tasks::members(tx, &target.input.group_id)?;
    target.transition(
        if target.state == contracts::TaskState::Review {
            TaskAction::Return {
                reason: text.clone(),
            }
        } else {
            TaskAction::Supplement { text: text.clone() }
        },
        &members,
        now,
    )?;
    target
        .message_sequences
        .extend(source.message_sequences.iter().copied());
    project(tx, &mut target, start)?;
    save(tx, &target)
}
pub(crate) fn save(tx: &Transaction<'_>, task: &OutcomeTask) -> Result<()> {
    ensure!(
        task.message_sequences.len() <= 8192 && serde_json::to_vec(task)?.len() <= 4 * 1024 * 1024,
        "{}",
        contracts::TaskErrorCode::Budget
    );
    tx.execute(
        "UPDATE outcome_tasks SET kind=?2,active=?3,value=?4 WHERE id=?1",
        params![
            task.input.id,
            task.kind.as_str(),
            task.active(),
            serde_json::to_string(task)?
        ],
    )?;
    Ok(())
}
pub(crate) fn project(tx: &Transaction<'_>, task: &mut OutcomeTask, start: usize) -> Result<()> {
    use contracts::TaskEventKind as E;
    let events = task.events[start..].to_vec();
    for event in events {
        if matches!(event.kind, E::Claimed | E::BudgetChanged) {
            continue;
        }
        let node = event
            .node_id
            .as_ref()
            .and_then(|id| task.nodes.iter().find(|n| n.input.id == *id));
        let artifact = if event.kind == E::Submitted {
            node.and_then(|n| n.artifact_id.as_ref()).and_then(|id| {
                task.artifacts
                    .iter()
                    .find(|a| a.id == *id && a.attempt == node.unwrap().attempt)
            })
        } else {
            None
        };
        let person = artifact.map(|a| a.author_id.clone());
        let text = match event.kind {
            E::Planned | E::Revised => task
                .nodes
                .iter()
                .map(|n| {
                    format!(
                        "{}: {} — {}",
                        n.input.person_id, n.input.title, n.input.deliverable
                    )
                })
                .collect::<Vec<_>>()
                .join("\n"),
            E::Delivered => task
                .deliveries
                .last()
                .map(|d| d.summary.clone())
                .unwrap_or_default(),
            E::Submitted => artifact.map(|a| a.summary.clone()).unwrap_or_default(),
            E::Clarification
            | E::Responded
            | E::Attached
            | E::Checked
            | E::Supplemented
            | E::Returned => event.text.clone(),
            _ => String::new(),
        };
        // A failed attempt gets a localized event, never a raw private diagnostic.
        let mut body = text.as_str();
        let mut part = 0;
        loop {
            let mut end = body.len().min(MAX_TEXT);
            while !body.is_char_boundary(end) {
                end -= 1;
            }
            let content = &body[..end];
            let count =
                tx.query_row("SELECT COUNT(*) FROM messages", [], |r| r.get::<_, u64>(0))?;
            let receipt = store::append(
                tx,
                person.as_deref().unwrap_or("operator"),
                &Publish {
                    group_id: task.input.group_id.clone(),
                    request_id: format!(
                        "task-{}-{}-{part}",
                        &storage::digest(task.input.id.as_bytes())[..24],
                        event.sequence
                    ),
                    text: if content.is_empty() {
                        event.kind.as_str().into()
                    } else {
                        content.into()
                    },
                    reply_to: task.reply_to,
                },
                &mut MAX_MESSAGES.saturating_sub(count),
                true,
            )?;
            let metadata = TaskMessage {
                task_id: task
                    .linked_task_id
                    .clone()
                    .unwrap_or_else(|| task.input.id.clone()),
                kind: event.kind,
                title: task.title.clone(),
                node_id: event.node_id.clone(),
                person_id: person.clone(),
                has_content: !content.is_empty(),
            };
            tx.execute(
                "UPDATE messages SET event=?2 WHERE sequence=?1",
                params![receipt.message.sequence, serde_json::to_string(&metadata)?],
            )?;
            task.message_sequences.push(receipt.message.sequence);
            body = &body[end..];
            part += 1;
            if body.is_empty() {
                break;
            }
            ensure!(part < 16, "{}", contracts::TaskErrorCode::Budget);
        }
    }
    Ok(())
}
pub(crate) fn busy_until(db: &Connection) -> Result<BTreeMap<String, u64>> {
    let mut busy = BTreeMap::<String, u64>::new();
    let mut add = |person: String, deadline: u64| {
        busy.entry(person)
            .and_modify(|d| *d = (*d).max(deadline))
            .or_insert(deadline);
    };
    let mut q = db.prepare("SELECT value FROM outcome_tasks WHERE active=1")?;
    for raw in q.query_map([], |r| r.get::<_, String>(0))? {
        let task: OutcomeTask = serde_json::from_str(&raw?)?;
        for node in task.nodes {
            if node.state == contracts::TaskNodeState::Running
                && let Some(lease) = node.lease
            {
                add(node.input.person_id, lease.deadline_ms);
            }
        }
    }
    let mut q = db.prepare("SELECT value FROM goals WHERE active=1")?;
    for raw in q.query_map([], |r| r.get::<_, String>(0))? {
        let goal: Goal = serde_json::from_str(&raw?)?;
        for work in goal.work {
            if work.state == contracts::WorkState::Running
                && let Some(deadline) = work.deadline_ms
            {
                add(work.person_id, deadline);
            }
        }
    }
    Ok(busy)
}

#[cfg(test)]
#[path = "task_messages_tests.rs"]
mod tests;
