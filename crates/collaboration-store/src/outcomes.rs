//! Single-writer transactions enforce publication authority and immutable evidence.
use crate::*;
use contracts::{OutcomeErrorCode as Code, OutcomeEventKind as Event, *};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Serialize, de::DeserializeOwned};

mod validation;
mod verification;
use validation::*;

impl Hub {
    /// None is the trusted host boundary. Agent transports must supply their bearer grant.
    pub async fn outcomes(&self, token: Option<&str>, command: OutcomeCommand) -> Result<Value> {
        let (actor, credential) = match token {
            Some(token) => (
                self.authenticate(token)?,
                Some(storage::digest(token.as_bytes())),
            ),
            None => ("operator".into(), None),
        };
        self.control(Control::Outcome {
            actor,
            credential,
            command: Box::new(command),
        })
        .await
    }
}

pub(super) fn check(condition: bool, code: Code) -> Result<()> {
    ensure!(condition, "{code}");
    Ok(())
}

fn outcome(db: &Connection, id: &str) -> Result<Outcome> {
    let (sequence, raw): (u64, String) = db
        .query_row(
            "SELECT sequence,value FROM outcomes WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .context(Code::NotFound)?;
    let mut value: Outcome = serde_json::from_str(&raw)?;
    value.sequence = sequence;
    Ok(value)
}
fn authorize(db: &Connection, actor: &str, group_id: &str, write: bool) -> Result<String> {
    check(
        db.query_row(
            "SELECT EXISTS(SELECT 1 FROM members WHERE group_id=?1 AND actor_id=?2)",
            params![group_id, actor],
            |r| r.get(0),
        )?,
        Code::Forbidden,
    )?;
    if write {
        check(
            db.query_row(
                "SELECT EXISTS(SELECT 1 FROM groups WHERE id=?1 AND archived=0)",
                [group_id],
                |r| r.get(0),
            )?,
            Code::Conflict,
        )?;
    }
    Ok(group_id.into())
}
fn capacity(db: &Connection, table: &str, limit: u64) -> Result<()> {
    let count: u64 = db.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))?;
    check(count < limit, Code::Capacity)
}
fn event<T: Serialize>(
    db: &Connection,
    outcome_id: &str,
    kind: Event,
    id: &str,
    value: &T,
) -> Result<()> {
    capacity(db, "outcome_events", 500000)?;
    db.execute("INSERT INTO outcome_events(outcome_id,kind,request_id,value,created_ms) VALUES(?1,?2,?3,?4,?5)", params![outcome_id,kind.as_str(),id,serde_json::to_string(value)?,now_ms()])?;
    Ok(())
}
fn by_request<T: DeserializeOwned>(db: &Connection, kind: Event, id: &str) -> Result<Option<T>> {
    let raw: Option<String> = db
        .query_row(
            "SELECT value FROM outcome_events WHERE kind=?1 AND request_id=?2",
            params![kind.as_str(), id],
            |r| r.get(0),
        )
        .optional()?;
    raw.map(|value| serde_json::from_str(&value).map_err(Into::into))
        .transpose()
}
fn latest<T: DeserializeOwned>(db: &Connection, kind: Event, id: &str) -> Result<Option<T>> {
    let raw: Option<String> = db.query_row("SELECT value FROM outcome_events WHERE kind=?1 AND outcome_id=?2 ORDER BY sequence DESC LIMIT 1", params![kind.as_str(),id], |r|r.get(0)).optional()?;
    raw.map(|value| serde_json::from_str(&value).map_err(Into::into))
        .transpose()
}
fn digest(value: &impl Serialize) -> Result<String> {
    Ok(storage::digest(&serde_json::to_vec(value)?))
}

impl Store {
    pub(crate) fn outcome_command(
        &mut self,
        actor: &str,
        credential: Option<&str>,
        command: OutcomeCommand,
    ) -> Result<Value> {
        let tx = self.connection.transaction()?;
        if let Some(hash) = credential {
            check(command.agent_allowed(), Code::Forbidden)?;
            check(tx.query_row("SELECT EXISTS(SELECT 1 FROM grants WHERE token_hash=?1 AND actor_id=?2 AND expires_ms>?3)", params![hash,actor,now_ms()], |r|r.get(0))?, Code::Forbidden)?;
        } else {
            check(actor == "operator", Code::Forbidden)?;
        }
        let result = match command {
            OutcomeCommand::Publish { input } => publish(&tx, actor, credential.is_some(), *input)?,
            OutcomeCommand::Search { query } => search(&tx, actor, &query)?,
            OutcomeCommand::Get { id } => {
                let entry = outcome(&tx, &id)?;
                authorize(&tx, actor, &entry.input.group_id, false)?;
                json!(view(&tx, entry)?)
            }
            OutcomeCommand::History { id, after } => {
                let entry = outcome(&tx, &id)?;
                authorize(&tx, actor, &entry.input.group_id, false)?;
                check(after <= i64::MAX as u64, Code::Invalid)?;
                let mut query = tx.prepare("SELECT sequence,kind,value,created_ms FROM outcome_events WHERE outcome_id=?1 AND sequence>?2 ORDER BY sequence LIMIT 33")?;
                let mut events = query.query_map(params![id,after], |r|Ok((r.get::<_,u64>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,u64>(3)?)))?.map(|row| {
                    let (sequence,kind,raw,created_ms) = row?;
                    Ok(json!({"sequence":sequence,"kind":Event::parse(&kind)?,"record":serde_json::from_str::<Value>(&raw)?,"created_ms":created_ms}))
                }).collect::<Result<Vec<_>>>()?;
                let next = (events.len() > OUTCOME_PAGE_SIZE)
                    .then(|| events[OUTCOME_PAGE_SIZE - 1]["sequence"].clone());
                events.truncate(OUTCOME_PAGE_SIZE);
                json!({"events":events,"next_after":next})
            }
            OutcomeCommand::Withdraw { id, reason } => {
                bounded(&reason, 2048)?;
                let entry = outcome(&tx, &id)?;
                authorize(&tx, actor, &entry.input.group_id, true)?;
                if let Some(old) = by_request::<String>(&tx, Event::Withdrawn, &id)? {
                    check(old == reason, Code::Conflict)?;
                } else {
                    event(&tx, &id, Event::Withdrawn, &id, &reason)?;
                }
                json!(view(&tx, entry)?)
            }
            other => verification::change(&tx, actor, other)?,
        };
        tx.commit()?;
        Ok(result)
    }
}

fn publish(db: &Connection, actor: &str, agent: bool, input: OutcomeInput) -> Result<Value> {
    validate_input(&input)?;
    check(input.node_id.is_none(), Code::Forbidden)?;
    let owner = authorize(db, actor, &input.group_id, true)?;
    if db.query_row(
        "SELECT EXISTS(SELECT 1 FROM outcomes WHERE id=?1)",
        [&input.id],
        |r| r.get::<_, bool>(0),
    )? {
        let old = outcome(db, &input.id)?;
        check(old.input == input && old.author_id == actor, Code::Conflict)?;
        return Ok(json!(view(db, old)?));
    }
    let goal = goals::read(db, &input.goal_id)?;
    check(goal.input.group_id == owner, Code::Forbidden)?;
    if agent {
        let binding = input.attempt.as_ref().context(Code::StaleAttempt)?;
        check(
            binding.goal_id == input.goal_id && goal.state == GoalState::Active,
            Code::StaleAttempt,
        )?;
        let work = goal
            .work
            .iter()
            .find(|w| w.id == binding.work_id && w.person_id == actor)
            .context(Code::Forbidden)?;
        check(
            work.attempt == binding.attempt
                && work.claim_id.as_deref() == Some(&binding.claim_id)
                && work.state == WorkState::Running
                && work.deadline_ms.is_some_and(|d| d > now_ms()),
            Code::StaleAttempt,
        )?;
    } else {
        check(input.attempt.is_none(), Code::Invalid)?;
    }
    for parent in input
        .references
        .iter()
        .map(|r| &r.outcome_id)
        .chain(input.supersedes.iter())
    {
        let previous = outcome(db, parent)?;
        check(previous.input.group_id == input.group_id, Code::Forbidden)?;
        if input.supersedes.as_ref() == Some(parent) {
            check(previous.author_id == actor || !agent, Code::Forbidden)?;
        }
    }
    capacity(db, "outcomes", 100000)?;
    let entry = Outcome {
        digest: digest(&(actor, &input))?,
        input,
        author_id: actor.into(),
        created_ms: now_ms(),
        sequence: 0,
    };
    db.execute("INSERT INTO outcomes(id,group_id,goal_id,author_id,kind,title,summary,value) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)", params![entry.input.id,entry.input.group_id,entry.input.goal_id,actor,entry.input.kind.as_str(),entry.input.title,entry.input.summary,serde_json::to_string(&entry)?])?;
    for reference in &entry.input.references {
        db.execute(
            "INSERT INTO outcome_references(child_id,parent_id,kind) VALUES(?1,?2,?3)",
            params![
                entry.input.id,
                reference.outcome_id,
                reference.kind.as_str()
            ],
        )?;
    }
    Ok(json!(view(db, outcome(db, &entry.input.id)?)?))
}

fn search(db: &Connection, actor: &str, input: &OutcomeQuery) -> Result<Value> {
    authorize(db, actor, &input.group_id, false)?;
    check(
        input.query.len() <= 256 && input.goal_id.len() <= 128 && input.after <= 100000,
        Code::Invalid,
    )?;
    let snapshot = input.snapshot.unwrap_or(db.query_row(
        "SELECT COALESCE(MAX(sequence),0) FROM outcomes",
        [],
        |r| r.get(0),
    )?);
    check(snapshot <= i64::MAX as u64, Code::Invalid)?;
    // Rank only visible, already-published references inside this frozen result window.
    let mut query = db.prepare("SELECT o.id, (SELECT COUNT(*) FROM outcome_references r JOIN outcomes child ON child.id=r.child_id WHERE r.parent_id=o.id AND r.kind=?7 AND child.sequence<=?5) AS uses FROM outcomes o WHERE o.group_id=?1 AND (?2='' OR o.goal_id=?2) AND (?3='' OR o.kind=?3) AND (?4='' OR instr(lower(o.title),lower(?4))>0 OR instr(lower(o.summary),lower(?4))>0) AND o.sequence<=?5 ORDER BY CASE WHEN ?4<>'' AND instr(lower(o.title),lower(?4))>0 THEN 1 ELSE 0 END DESC, MIN(uses,8) DESC, o.sequence DESC LIMIT 33 OFFSET ?6")?;
    let mut entries = query
        .query_map(
            params![
                input.group_id,
                input.goal_id,
                input.kind.map(|k| k.as_str()).unwrap_or(""),
                input.query,
                snapshot,
                input.after,
                OutcomeReferenceKind::Uses.as_str()
            ],
            |r| r.get::<_, String>(0),
        )?
        .map(|id| {
            let entry = view(db, outcome(db, &id?)?)?;
            let input = &entry.outcome.input;
            Ok(
                json!({"id":input.id,"group_id":input.group_id,"goal_id":input.goal_id,
                "kind":input.kind,"title":input.title,"summary":input.summary,
                "author_id":entry.outcome.author_id,"created_ms":entry.outcome.created_ms,
                "publication":entry.publication,"verification":entry.verification,
                "acceptance":entry.acceptance,"reference_count":entry.reference_count}),
            )
        })
        .collect::<Result<Vec<_>>>()?;
    let next =
        (entries.len() > OUTCOME_PAGE_SIZE).then_some(input.after + OUTCOME_PAGE_SIZE as u64);
    entries.truncate(OUTCOME_PAGE_SIZE);
    Ok(json!({"outcomes":entries,"snapshot":snapshot,"next_after":next,"summary_bytes_limit":4096}))
}

fn view(db: &Connection, entry: Outcome) -> Result<OutcomeView> {
    let id = &entry.input.id;
    let withdrawal_reason = by_request::<String>(db, Event::Withdrawn, id)?;
    let mut latest_verification =
        latest::<OutcomeVerification>(db, Event::VerificationStarted, id)?;
    if let Some(run) = &mut latest_verification {
        run.result = by_request(db, Event::VerificationFinished, &run.input.id)?;
    }
    let mut verification =
        latest_verification
            .as_ref()
            .map_or(OutcomeVerificationState::Unverified, |run| {
                match run.result.as_ref().map(|result| result.state) {
                    Some(VerificationRunState::Passed) => OutcomeVerificationState::Passed,
                    Some(VerificationRunState::Failed) => OutcomeVerificationState::Failed,
                    Some(VerificationRunState::Unavailable) => {
                        OutcomeVerificationState::Unavailable
                    }
                    Some(VerificationRunState::Unknown) => OutcomeVerificationState::Unknown,
                    _ if run.deadline_ms <= now_ms() => OutcomeVerificationState::Unknown,
                    _ => OutcomeVerificationState::Running,
                }
            });
    if !current_attempt(db, &entry)? {
        verification = OutcomeVerificationState::Unverified;
    }
    let latest_decision = latest::<OutcomeDecision>(db, Event::Decision, id)?;
    let acceptance = latest_decision
        .as_ref()
        .map_or(OutcomeAcceptanceState::Pending, |decision| {
            if decision.state == OutcomeAcceptanceState::Returned {
                return decision.state;
            }
            if verification == OutcomeVerificationState::Passed
                && withdrawal_reason.is_none()
                && latest_verification
                    .as_ref()
                    .is_some_and(|run| decision.verification_id.as_ref() == Some(&run.input.id))
            {
                decision.state
            } else {
                OutcomeAcceptanceState::Pending
            }
        });
    let mut latest_write = latest::<KnowledgeWrite>(db, Event::WriteStarted, id)?;
    if let Some(write) = &mut latest_write {
        write.result = verification::write_result(db, &write.input.id)?;
    }
    let reference_count = db.query_row(
        "SELECT COUNT(*) FROM outcome_references WHERE parent_id=?1 AND kind=?2",
        params![id, OutcomeReferenceKind::Uses.as_str()],
        |r| r.get(0),
    )?;
    Ok(OutcomeView {
        outcome: entry,
        publication: if withdrawal_reason.is_some() {
            OutcomePublicationState::Withdrawn
        } else {
            OutcomePublicationState::Published
        },
        withdrawal_reason,
        verification,
        acceptance,
        latest_verification,
        latest_decision,
        latest_write,
        reference_count,
    })
}

/// Node outputs are recorded atomically with the owning task, never via worker authority.
pub(crate) fn record_task_artifacts(db: &Connection, task: &OutcomeTask) -> Result<()> {
    if task.kind != TaskKind::Outcome {
        return Ok(());
    }
    for artifact in &task.artifacts {
        if db.query_row(
            "SELECT EXISTS(SELECT 1 FROM outcomes WHERE id=?1)",
            [&artifact.id],
            |r| r.get::<_, bool>(0),
        )? {
            continue;
        }
        let node = task
            .nodes
            .iter()
            .chain(task.retired_nodes.iter())
            .find(|n| n.input.id == artifact.node_id)
            .context(Code::Invalid)?;
        let run = node
            .attempts
            .iter()
            .find(|r| r.attempt == artifact.attempt)
            .context(Code::Invalid)?;
        let source = run.receipt["input_sha256"]
            .as_str()
            .context(Code::Invalid)?
            .to_owned();
        let evidence: Vec<OutcomeEvidence> =
            serde_json::from_value(artifact.evidence["repository_evidence"].clone())
                .unwrap_or_default();
        let evidence = if evidence.is_empty() || validate_evidence(&evidence).is_ok() {
            evidence
        } else {
            vec![]
        };
        let input = OutcomeInput {
            node_id: Some(artifact.node_id.clone()),
            id: artifact.id.clone(),
            group_id: task.input.group_id.clone(),
            goal_id: task.input.id.clone(),
            kind: OutcomeKind::Finding,
            title: node.input.title.clone(),
            summary: artifact.summary.clone(),
            conditions: node.input.acceptance.clone(),
            evidence,
            references: artifact
                .inputs
                .iter()
                .map(|id| OutcomeReference {
                    outcome_id: id.clone(),
                    kind: OutcomeReferenceKind::Uses,
                    reason: "Upstream task artifact".into(),
                })
                .collect(),
            supersedes: None,
            attempt: Some(OutcomeAttempt {
                goal_id: task.input.id.clone(),
                work_id: artifact.node_id.clone(),
                attempt: artifact.attempt,
                claim_id: run.claim_id.clone(),
            }),
            pins: EvidencePins {
                source: Some(source),
                prompt: Some(storage::digest(node.input.instruction.as_bytes())),
                policy: Some(storage::digest(b"outcome-task-protocol-v1")),
                superpod: None,
            },
            knowledge: None,
        };
        validate_input(&input)?;
        let entry = Outcome {
            digest: digest(&(&artifact.author_id, &input))?,
            input,
            author_id: artifact.author_id.clone(),
            created_ms: artifact.created_ms,
            sequence: 0,
        };
        db.execute("INSERT INTO outcomes(id,group_id,goal_id,author_id,kind,title,summary,value) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![entry.input.id,entry.input.group_id,entry.input.goal_id,entry.author_id,entry.input.kind.as_str(),entry.input.title,entry.input.summary,serde_json::to_string(&entry)?])?;
        for reference in &entry.input.references {
            db.execute(
                "INSERT INTO outcome_references(child_id,parent_id,kind) VALUES(?1,?2,?3)",
                params![
                    entry.input.id,
                    reference.outcome_id,
                    reference.kind.as_str()
                ],
            )?;
        }
    }
    Ok(())
}
