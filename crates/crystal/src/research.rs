//! Transactional topic registry and append-only population lineage.
use crate::*;
use contracts::{EvolutionOperation, EvolutionState, TopicState};
use rusqlite::{Connection, OptionalExtension, params};
pub(crate) enum ResearchCommand {
    Change(Box<ResearchChange>),
    Topics {
        after: String,
        query: String,
    },
    Subjects {
        topic: String,
        after: String,
    },
    Graph {
        topic: String,
        subject: String,
        after: u64,
    },
}
impl Hub {
    pub async fn research_change(&self, change: ResearchChange) -> Result<Value> {
        self.control(Control::Research(ResearchCommand::Change(Box::new(change))))
            .await
    }
    pub async fn topics(&self, after: &str, query: &str) -> Result<Value> {
        self.control(Control::Research(ResearchCommand::Topics {
            after: after.into(),
            query: query.into(),
        }))
        .await
    }
    pub async fn subjects(&self, topic: &str, after: &str) -> Result<Value> {
        self.control(Control::Research(ResearchCommand::Subjects {
            topic: topic.into(),
            after: after.into(),
        }))
        .await
    }
    pub async fn graph(&self, topic: &str, subject: &str, after: u64) -> Result<Value> {
        self.control(Control::Research(ResearchCommand::Graph {
            topic: topic.into(),
            subject: subject.into(),
            after,
        }))
        .await
    }
}
fn topic(db: &Connection, id: &str) -> Result<ResearchTopic> {
    let value: String =
        db.query_row("SELECT value FROM research_topics WHERE id=?1", [id], |r| {
            r.get(0)
        })?;
    Ok(serde_json::from_str(&value)?)
}
fn subject(db: &Connection, id: &str) -> Result<ResearchSubject> {
    let value: String = db.query_row(
        "SELECT value FROM research_subjects WHERE id=?1",
        [id],
        |r| r.get(0),
    )?;
    Ok(serde_json::from_str(&value)?)
}
fn node(db: &Connection, id: &str) -> Result<EvolutionNode> {
    let value: String =
        db.query_row("SELECT value FROM research_nodes WHERE id=?1", [id], |r| {
            r.get(0)
        })?;
    Ok(serde_json::from_str(&value)?)
}
fn reference(value: &str) -> Result<()> {
    text(value, 2048)?;
    ensure!(!value.trim().is_empty(), "research reference required");
    Ok(())
}
fn capacity(db: &Connection, table: &str, max: u64) -> Result<()> {
    let count: u64 = db.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))?;
    ensure!(count < max, "research registry capacity exceeded");
    Ok(())
}
impl Store {
    pub(crate) fn research_command(&mut self, request: ResearchCommand) -> Result<Value> {
        match request {
            ResearchCommand::Change(change) => self.research_change(*change),
            ResearchCommand::Topics { after, query } => {
                ensure!(
                    query.len() <= 256 && after.len() <= 128,
                    "topic query exceeds bound"
                );
                let mut statement=self.connection.prepare("SELECT value,(SELECT COUNT(*) FROM research_subjects s WHERE s.topic_id=t.id),(SELECT COUNT(*) FROM goals g WHERE g.group_id=t.group_id) FROM research_topics t WHERE id>?1 AND (lower(json_extract(value,'$.title')) LIKE ?2 OR lower(json_extract(value,'$.objective')) LIKE ?2) ORDER BY id LIMIT 65")?;
                let mut rows = Vec::new();
                for row in statement.query_map(
                    params![after, format!("%{}%", query.to_lowercase())],
                    |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, u64>(1)?,
                            r.get::<_, u64>(2)?,
                        ))
                    },
                )? {
                    let (value, subjects, goals) = row?;
                    let topic: ResearchTopic = serde_json::from_str(&value)?;
                    rows.push(json!({"topic":topic,"subject_count":subjects,"goal_count":goals}));
                }
                let next = (rows.len() > 64).then(|| rows[63]["topic"]["id"].clone());
                rows.truncate(64);
                Ok(json!({"topics":rows,"next_after":next}))
            }
            ResearchCommand::Subjects { topic, after } => {
                ensure!(
                    topic.len() <= 128 && after.len() <= 128,
                    "subject query exceeds bound"
                );
                let mut statement=self.connection.prepare("SELECT value,(SELECT COUNT(*) FROM research_nodes n WHERE n.subject_id=s.id) FROM research_subjects s WHERE id>?1 AND (?2='' OR topic_id=?2) ORDER BY id LIMIT 65")?;
                let mut rows = Vec::new();
                for row in statement.query_map(params![after, topic], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, u64>(1)?))
                })? {
                    let (value, count) = row?;
                    let subject: ResearchSubject = serde_json::from_str(&value)?;
                    rows.push(json!({"subject":subject,"node_count":count}));
                }
                let next = (rows.len() > 64).then(|| rows[63]["subject"]["id"].clone());
                rows.truncate(64);
                Ok(json!({"subjects":rows,"next_after":next}))
            }
            ResearchCommand::Graph {
                topic,
                subject,
                after,
            } => self.research_graph(&topic, &subject, after),
        }
    }
    fn research_change(&mut self, change: ResearchChange) -> Result<Value> {
        let tx = self.connection.transaction()?;
        let value = match change {
            ResearchChange::Assessment { assessment: input } => {
                id(&input.id)?;
                id(&input.node_id)?;
                id(&input.goal_id)?;
                if let Some(value) = tx
                    .query_row(
                        "SELECT value FROM research_assessments WHERE id=?1",
                        [&input.id],
                        |r| r.get::<_, String>(0),
                    )
                    .optional()?
                {
                    let existing: VersionAssessment = serde_json::from_str(&value)?;
                    ensure!(existing.input == input, "version assessment is immutable");
                    return Ok(json!(existing));
                }
                let version = node(&tx, &input.node_id)?;
                let owner = subject(&tx, &version.input.subject_id)?;
                let topic = topic(&tx, &owner.topic_id)?;
                let goal = goals::read(&tx, &input.goal_id)?;
                ensure!(
                    goal.input.group_id == topic.group_id,
                    "assessment goal belongs to another group"
                );
                ensure!(
                    input.goal_attempts.len() == goal.work.len()
                        && input.pins_by_work.len() == goal.work.len()
                        && goal
                            .work
                            .iter()
                            .all(|w| input.goal_attempts.get(&w.id) == Some(&w.attempt)
                                && input.pins_by_work.contains_key(&w.id)),
                    "assessment attempts differ from current goal"
                );
                for pins in input.pins_by_work.values() {
                    for pin in [&pins.source, &pins.prompt, &pins.policy, &pins.superpod]
                        .into_iter()
                        .flatten()
                    {
                        ensure!(
                            [40, 64].contains(&pin.len())
                                && pin.bytes().all(|b| b.is_ascii_hexdigit()),
                            "invalid assessment pin"
                        );
                    }
                    ensure!(
                        version.input.pins.source.is_none()
                            || pins.source == version.input.pins.source,
                        "assessment source differs from version"
                    );
                }
                capacity(&tx, "research_assessments", 100000)?;
                let assessment = VersionAssessment {
                    input,
                    created_ms: now_ms(),
                };
                tx.execute("INSERT INTO research_assessments(id,node_id,goal_id,value) VALUES(?1,?2,?3,?4)", params![assessment.input.id, assessment.input.node_id, assessment.input.goal_id, serde_json::to_string(&assessment)?])?;
                json!(assessment)
            }
            ResearchChange::Topic { mut topic } => {
                id(&topic.id)?;
                id(&topic.group_id)?;
                text(&topic.title, 256)?;
                text(&topic.objective, 16000)?;
                ensure!(tx.query_row("SELECT EXISTS(SELECT 1 FROM groups WHERE id=?1 AND archived=0 AND kind<>'board')",[&topic.group_id],|r|r.get::<_,bool>(0))?,"topic requires an active conversation group");
                let old: Option<(u64, String)> = tx
                    .query_row(
                        "SELECT revision,group_id FROM research_topics WHERE id=?1",
                        [&topic.id],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                    .optional()?;
                ensure!(
                    topic.revision == old.as_ref().map_or(0, |v| v.0),
                    "topic revision conflict"
                );
                ensure!(
                    old.as_ref().is_none_or(|v| v.1 == topic.group_id),
                    "topic group is immutable"
                );
                if old.is_none() {
                    capacity(&tx, "research_topics", 2048)?;
                }
                topic.revision = topic
                    .revision
                    .checked_add(1)
                    .context("topic revision exhausted")?;
                tx.execute("INSERT INTO research_topics(id,group_id,revision,value) VALUES(?1,?2,?3,?4) ON CONFLICT(id) DO UPDATE SET revision=excluded.revision,value=excluded.value",params![topic.id,topic.group_id,topic.revision,serde_json::to_string(&topic)?])?;
                json!(topic)
            }
            ResearchChange::Subject { mut subject } => {
                id(&subject.id)?;
                id(&subject.topic_id)?;
                text(&subject.name, 256)?;
                reference(&subject.reference)?;
                ensure!(
                    topic(&tx, &subject.topic_id)?.state == TopicState::Active,
                    "topic is archived"
                );
                let old: Option<(u64, String)> = tx
                    .query_row(
                        "SELECT revision,topic_id FROM research_subjects WHERE id=?1",
                        [&subject.id],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                    .optional()?;
                ensure!(
                    subject.revision == old.as_ref().map_or(0, |v| v.0),
                    "subject revision conflict"
                );
                ensure!(
                    old.as_ref().is_none_or(|v| v.1 == subject.topic_id),
                    "subject topic is immutable"
                );
                if old.is_none() {
                    capacity(&tx, "research_subjects", 8192)?;
                }
                subject.revision = subject
                    .revision
                    .checked_add(1)
                    .context("subject revision exhausted")?;
                tx.execute("INSERT INTO research_subjects(id,topic_id,revision,value) VALUES(?1,?2,?3,?4) ON CONFLICT(id) DO UPDATE SET revision=excluded.revision,value=excluded.value",params![subject.id,subject.topic_id,subject.revision,serde_json::to_string(&subject)?])?;
                json!(subject)
            }
            ResearchChange::Node { node: input } => {
                id(&input.id)?;
                id(&input.subject_id)?;
                text(&input.version, 256)?;
                reference(&input.reference)?;
                ensure!(
                    input.parents.len() <= 4,
                    "a node supports at most four parents"
                );
                let owner = subject(&tx, &input.subject_id)?;
                let topic = topic(&tx, &owner.topic_id)?;
                ensure!(topic.state == TopicState::Active, "topic is archived");
                if tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM research_nodes WHERE id=?1)",
                    [&input.id],
                    |r| r.get::<_, bool>(0),
                )? {
                    let existing = node(&tx, &input.id)?;
                    ensure!(existing.input == input, "evolution node is immutable");
                    return Ok(json!(existing));
                }
                capacity(&tx, "research_nodes", 100000)?;
                for pin in [
                    &input.pins.source,
                    &input.pins.prompt,
                    &input.pins.policy,
                    &input.pins.superpod,
                ]
                .into_iter()
                .flatten()
                {
                    ensure!(
                        [40, 64].contains(&pin.len()) && pin.bytes().all(|b| b.is_ascii_hexdigit()),
                        "evidence pin must be a full commit or SHA-256 digest"
                    );
                }
                let goal_attempts = if let Some(goal_id) = &input.goal_id {
                    id(goal_id)?;
                    let goal = goals::read(&tx, goal_id)?;
                    ensure!(
                        goal.input.group_id == topic.group_id,
                        "evolution evidence goal belongs to another group"
                    );
                    Some(
                        goal.work
                            .iter()
                            .map(|work| (work.id.clone(), work.attempt))
                            .collect(),
                    )
                } else {
                    None
                };
                let mut parents = std::collections::BTreeSet::new();
                let mut generation = 0;
                for parent in &input.parents {
                    id(parent)?;
                    ensure!(parents.insert(parent), "duplicate evolution parent");
                    let parent = node(&tx, parent)?;
                    ensure!(
                        parent.input.subject_id == input.subject_id,
                        "evolution parents must describe the same research subject"
                    );
                    generation = generation.max(parent.generation + 1);
                }
                let node = EvolutionNode {
                    input,
                    generation,
                    created_ms: now_ms(),
                    goal_attempts,
                };
                tx.execute(
                    "INSERT INTO research_nodes(id,subject_id,goal_id,value) VALUES(?1,?2,?3,?4)",
                    params![
                        node.input.id,
                        node.input.subject_id,
                        node.input.goal_id,
                        serde_json::to_string(&node)?
                    ],
                )?;
                for parent in &node.input.parents {
                    tx.execute(
                        "INSERT INTO research_edges(child_id,parent_id) VALUES(?1,?2)",
                        params![node.input.id, parent],
                    )?;
                }
                json!(node)
            }
        };
        tx.commit()?;
        Ok(value)
    }
    fn research_graph(&self, topic_id: &str, subject_id: &str, after: u64) -> Result<Value> {
        ensure!(
            topic_id.len() <= 128 && subject_id.len() <= 128 && after <= i64::MAX as u64,
            "graph query exceeds bound"
        );
        let mut statement=self.connection.prepare("SELECT n.sequence,n.value,s.value,t.value FROM research_nodes n JOIN research_subjects s ON n.subject_id=s.id JOIN research_topics t ON s.topic_id=t.id WHERE n.sequence>?1 AND (?2='' OR t.id=?2) AND (?3='' OR s.id=?3) ORDER BY n.sequence LIMIT 129")?;
        let mut rows = Vec::new();
        for row in statement.query_map(params![after, topic_id, subject_id], |r| {
            Ok((
                r.get::<_, u64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })? {
            let (sequence, value, subject, topic) = row?;
            let node: EvolutionNode = serde_json::from_str(&value)?;
            let subject: ResearchSubject = serde_json::from_str(&subject)?;
            let topic: ResearchTopic = serde_json::from_str(&topic)?;
            let goal = node
                .input
                .goal_id
                .as_ref()
                .map(|id| goals::read(&self.connection, id))
                .transpose()?;
            let mut status = match goal.as_ref() {
                Some(goal) => crate::iteration::assessment(&self.connection, &node, goal)?
                    .unwrap_or_else(|| assessment(&node, Some(goal))),
                None => assessment(&node, None),
            };
            let mut assessments = Vec::new();
            let mut query = self.connection.prepare("SELECT value FROM research_assessments WHERE node_id=?1 ORDER BY sequence DESC LIMIT 8")?;
            for value in query.query_map([&node.input.id], |r| r.get::<_, String>(0))? {
                let entry: VersionAssessment = serde_json::from_str(&value?)?;
                let evaluated = goals::read(&self.connection, &entry.input.goal_id)?;
                let verdict = version_assessment(&entry, &evaluated);
                if assessments.is_empty() {
                    status = verdict;
                }
                assessments.push(json!({"assessment":entry,"status":verdict,"goal_title":evaluated.input.title,"goal_revision":evaluated.revision}));
            }
            let assessment_count: u64 = self.connection.query_row(
                "SELECT COUNT(*) FROM research_assessments WHERE node_id=?1",
                [&node.input.id],
                |r| r.get(0),
            )?;
            let operation = match node.input.parents.len() {
                0 => EvolutionOperation::Baseline,
                1 => EvolutionOperation::Mutation,
                _ => EvolutionOperation::Recombination,
            };
            rows.push(json!({"sequence":sequence,"node":node,"subject":subject,"topic":topic,"status":status,"operation":operation,"goal":goal,"assessments":assessments,"assessment_count":assessment_count}));
        }
        let next = (rows.len() > 128).then(|| rows[127]["sequence"].clone());
        rows.truncate(128);
        Ok(json!({"nodes":rows,"next_after":next}))
    }
}

pub(super) fn version_assessment(assessment: &VersionAssessment, goal: &Goal) -> EvolutionState {
    if assessment.input.goal_attempts.len() != goal.work.len()
        || goal
            .work
            .iter()
            .any(|work| assessment.input.goal_attempts.get(&work.id) != Some(&work.attempt))
    {
        return EvolutionState::Unassessed;
    }
    if goal.state == GoalState::Cancelled {
        return EvolutionState::Cancelled;
    }
    if goal.work.iter().any(|w| w.state == WorkState::Failed) {
        return EvolutionState::Failed;
    }
    if goal.work.iter().any(|w| w.state == WorkState::Running) {
        return EvolutionState::Running;
    }
    if goal.work.iter().any(|w| w.state == WorkState::Ready) {
        return EvolutionState::Pending;
    }
    let bound = goal.work.iter().all(|work| {
        let Some(pins) = assessment.input.pins_by_work.get(&work.id) else {
            return false;
        };
        let Some(result) = &work.result else {
            return false;
        };
        result.succeeded
            && [
                (&pins.source, "source_commit"),
                (&pins.prompt, "prompt_digest"),
                (&pins.policy, "policy_digest"),
                (&pins.superpod, "superpod_commit"),
            ]
            .into_iter()
            .all(|(pin, field)| {
                pin.as_deref()
                    .is_some_and(|pin| result.evidence[field].as_str() == Some(pin))
            })
    });
    if !bound {
        EvolutionState::Unassessed
    } else if goal.state == GoalState::Completed {
        EvolutionState::Accepted
    } else {
        EvolutionState::Submitted
    }
}

/// Goal acceptance assesses only a version with matching, committed evidence.
fn evidence_matches(pins: &EvidencePins, goal: &Goal) -> bool {
    if goal.input.scenario != "research" {
        return false;
    }
    goal.work.iter().any(|work| {
        let Some(result) = &work.result else {
            return false;
        };
        work.state == WorkState::Submitted
            && result.succeeded
            && result.evidence["receipt_protocol"] == 2
            && result.evidence["receipt_sha256"]
                .as_str()
                .is_some_and(|v| v.len() == 64 && v.bytes().all(|b| b.is_ascii_hexdigit()))
            && [
                (&pins.source, "source_commit"),
                (&pins.prompt, "prompt_digest"),
                (&pins.policy, "policy_digest"),
                (&pins.superpod, "superpod_commit"),
            ]
            .into_iter()
            .all(|(pin, field)| {
                pin.as_deref()
                    .is_some_and(|pin| result.evidence[field].as_str() == Some(pin))
            })
    })
}

pub(super) fn assessment(node: &EvolutionNode, goal: Option<&Goal>) -> EvolutionState {
    match goal {
        None => EvolutionState::Unassessed,
        Some(g)
            if node.goal_attempts.as_ref().is_none_or(|attempts| {
                attempts.len() != g.work.len()
                    || g.work
                        .iter()
                        .any(|w| attempts.get(&w.id) != Some(&w.attempt))
            }) =>
        {
            EvolutionState::Unassessed
        }
        Some(g) => match g.state {
            GoalState::Completed if evidence_matches(&node.input.pins, g) => {
                EvolutionState::Accepted
            }
            GoalState::Completed => EvolutionState::Unassessed,
            GoalState::Cancelled => EvolutionState::Cancelled,
            GoalState::Active => {
                if g.work.iter().any(|w| w.state == WorkState::Failed) {
                    EvolutionState::Failed
                } else if g.work.iter().all(|w| w.state == WorkState::Submitted) {
                    if evidence_matches(&node.input.pins, g) {
                        EvolutionState::Submitted
                    } else {
                        EvolutionState::Unassessed
                    }
                } else if g.work.iter().any(|w| w.state == WorkState::Running) {
                    EvolutionState::Running
                } else {
                    EvolutionState::Pending
                }
            }
        },
    }
}
