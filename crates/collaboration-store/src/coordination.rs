//! Transactional topic and subject coordination registry.
use crate::*;
use contracts::TopicState;
use rusqlite::{Connection, OptionalExtension, params};
pub(crate) enum ResearchCommand {
    Change(Box<ResearchChange>),
    Topics { after: String, query: String },
    Subjects { topic: String, after: String },
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
}
fn topic(db: &Connection, id: &str) -> Result<CollaborationTopic> {
    let value: String =
        db.query_row("SELECT value FROM research_topics WHERE id=?1", [id], |r| {
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
                    let topic: CollaborationTopic = serde_json::from_str(&value)?;
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
                let mut statement=self.connection.prepare("SELECT value FROM research_subjects WHERE id>?1 AND (?2='' OR topic_id=?2) ORDER BY id LIMIT 65")?;
                let mut rows = Vec::new();
                for row in statement.query_map(params![after, topic], |r| r.get::<_, String>(0))? {
                    let value = row?;
                    let subject: CollaborationSubject = serde_json::from_str(&value)?;
                    rows.push(json!({"subject":subject}));
                }
                let next = (rows.len() > 64).then(|| rows[63]["subject"]["id"].clone());
                rows.truncate(64);
                let topic_exists = topic.is_empty()
                    || self.connection.query_row(
                        "SELECT EXISTS(SELECT 1 FROM research_topics WHERE id=?1)",
                        [&topic],
                        |r| r.get::<_, bool>(0),
                    )?;
                Ok(json!({"subjects":rows,"next_after":next,"topic_exists":topic_exists}))
            }
        }
    }
    fn research_change(&mut self, change: ResearchChange) -> Result<Value> {
        let tx = self.connection.transaction()?;
        let value = match change {
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
        };
        tx.commit()?;
        Ok(value)
    }
}
