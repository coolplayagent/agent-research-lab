//! Ad-hoc group DMs never expire. Expanding the audience forks a conversation;
//! conversion to a managed group preserves the original log and replay cursors.
use crate::{model::*, store::Store};
use anyhow::{Result, ensure};
use rusqlite::params;
use std::collections::{BTreeMap, BTreeSet};
impl Store {
    pub fn convert(&mut self, group: &str, revision: u64, title: &str) -> Result<()> {
        id(group)?;
        text(title, 256)?;
        ensure!(self.connection.execute("UPDATE groups SET kind='conversation',private=0,title=?1,revision=revision+1 WHERE id=?2 AND revision=?3 AND kind='temporary' AND archived=0", params![title, group, revision])? == 1, "temporary conversation changed or is closed; refresh before converting");
        Ok(())
    }
    pub fn fork(
        &mut self,
        source: &str,
        group: &NewGroup,
        history_from: Option<u64>,
    ) -> Result<()> {
        id(source)?;
        group.validate()?;
        ensure!(
            group.kind == GroupKind::Temporary && group.private,
            "new audience requires a temporary conversation"
        );
        let tx = self.connection.transaction()?;
        let (private, archived): (bool, bool) = tx.query_row(
            "SELECT private,archived FROM groups WHERE id=?1",
            [source],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        ensure!(
            private && !archived,
            "only an active direct conversation can be expanded"
        );
        let existing = tx
            .prepare("SELECT actor_id FROM members WHERE group_id=?1")?
            .query_map([source], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<BTreeSet<_>>>()?;
        let chosen: BTreeSet<_> = group.members.iter().cloned().collect();
        ensure!(
            existing.is_subset(&chosen) && chosen.len() > existing.len(),
            "include the existing audience and at least one new member"
        );
        ensure!(
            tx.query_row("SELECT COUNT(*) FROM groups", [], |r| r.get::<_, usize>(0))? < MAX_GROUPS,
            "group capacity exhausted"
        );
        ensure!(
            tx.query_row("SELECT COUNT(*) FROM members", [], |r| r.get::<_, usize>(0))?
                + chosen.len()
                <= MAX_MEMBERSHIPS,
            "membership capacity exhausted"
        );
        tx.execute("INSERT INTO groups(id,title,topic,private,kind,created_ms) VALUES(?1,?2,?3,1,'temporary',?4)", params![group.id, group.title, group.topic, now_ms()])?;
        for actor in chosen {
            tx.execute(
                "INSERT INTO members(group_id,actor_id) VALUES(?1,?2)",
                params![group.id, actor],
            )?;
        }
        if let Some(from) = history_from {
            ensure!(from <= i64::MAX as u64, "invalid history cursor");
            let mut query = tx.prepare("SELECT sequence,sender_id,text,reply_to,accepted_ms FROM messages WHERE group_id=?1 AND sequence>=?2 ORDER BY sequence LIMIT 10001")?;
            let messages = query
                .query_map(params![source, from], |r| {
                    Ok((
                        r.get::<_, u64>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, Option<u64>>(3)?,
                        r.get::<_, u64>(4)?,
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            ensure!(
                messages.len() <= 10000,
                "history copy exceeds 10000 messages; choose a later starting message"
            );
            ensure!(
                tx.query_row("SELECT COUNT(*) FROM messages", [], |r| r.get::<_, u64>(0))?
                    + messages.len() as u64
                    <= MAX_MESSAGES,
                "message capacity exhausted"
            );
            let mut sequences = BTreeMap::new();
            let mut last = 0;
            for (sequence, sender, content, reply, accepted) in messages {
                let parent = reply.and_then(|id| sequences.get(&id).copied());
                tx.execute("INSERT INTO messages(group_id,sender_id,request_id,text,reply_to,accepted_ms) VALUES(?1,?2,?3,?4,?5,?6)", params![group.id, sender, format!("forward-{sequence}"), content, parent, accepted])?;
                last = tx.last_insert_rowid() as u64;
                sequences.insert(sequence, last);
            }
            tx.execute(
                "UPDATE groups SET last_sequence=?1 WHERE id=?2",
                params![last, group.id],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
}
