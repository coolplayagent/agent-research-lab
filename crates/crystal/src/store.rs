//! SQLite is the durable authority. No acknowledgement or live publication precedes commit.
use crate::model::*;
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    time::Duration,
};

pub(crate) struct Catalog {
    pub groups: BTreeMap<String, Group>,
    pub members: BTreeMap<String, BTreeSet<String>>,
    pub actors: BTreeMap<String, Presence>,
    pub grants: BTreeMap<String, (String, u64)>,
    pub messages: u64,
}
pub(crate) struct Store {
    pub connection: Connection,
}
impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir_all(path.parent().context("database needs a parent")?)?;
        std::fs::set_permissions(
            path.parent().unwrap(),
            std::fs::Permissions::from_mode(0o700),
        )?;
        let connection = Connection::open(path)?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        connection.busy_timeout(Duration::from_millis(250))?;
        connection.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON; PRAGMA wal_autocheckpoint=1000;
          CREATE TABLE IF NOT EXISTS crystal_meta(key TEXT PRIMARY KEY,value INTEGER NOT NULL);
          INSERT OR IGNORE INTO crystal_meta VALUES('schema',1);
          CREATE TABLE IF NOT EXISTS actors(id TEXT PRIMARY KEY,presence TEXT NOT NULL DEFAULT 'online');
          CREATE TABLE IF NOT EXISTS groups(id TEXT PRIMARY KEY,title TEXT NOT NULL,topic TEXT NOT NULL,private INTEGER NOT NULL,archived INTEGER NOT NULL DEFAULT 0,pinned INTEGER NOT NULL DEFAULT 0,revision INTEGER NOT NULL DEFAULT 1,created_ms INTEGER NOT NULL,last_sequence INTEGER NOT NULL DEFAULT 0);
          CREATE TABLE IF NOT EXISTS members(group_id TEXT NOT NULL REFERENCES groups(id),actor_id TEXT NOT NULL REFERENCES actors(id),cursor INTEGER NOT NULL DEFAULT 0,PRIMARY KEY(group_id,actor_id));
          CREATE INDEX IF NOT EXISTS memberships ON members(actor_id,group_id);
          CREATE TABLE IF NOT EXISTS grants(token_hash TEXT PRIMARY KEY,actor_id TEXT NOT NULL REFERENCES actors(id),expires_ms INTEGER NOT NULL);
          CREATE INDEX IF NOT EXISTS grants_actor ON grants(actor_id);
          CREATE INDEX IF NOT EXISTS grants_expiry ON grants(expires_ms);
          CREATE TABLE IF NOT EXISTS messages(sequence INTEGER PRIMARY KEY AUTOINCREMENT,group_id TEXT NOT NULL REFERENCES groups(id),sender_id TEXT NOT NULL REFERENCES actors(id),request_id TEXT NOT NULL,text TEXT NOT NULL,reply_to INTEGER,accepted_ms INTEGER NOT NULL,UNIQUE(group_id,sender_id,request_id));
          CREATE INDEX IF NOT EXISTS room_history ON messages(group_id,sequence);")?;
        ensure!(
            connection.query_row(
                "SELECT value FROM crystal_meta WHERE key='schema'",
                [],
                |r| r.get::<_, u64>(0)
            )? == 1,
            "unsupported crystal schema"
        );
        ensure!(
            connection.query_row("PRAGMA synchronous", [], |r| r.get::<_, u64>(0))? == 2,
            "durable mode must remain FULL"
        );
        ensure!(
            connection.query_row("PRAGMA journal_mode", [], |r| r.get::<_, String>(0))? == "wal",
            "WAL journal mode is required"
        );
        ensure!(
            connection.query_row("PRAGMA foreign_keys", [], |r| r.get::<_, u64>(0))? == 1,
            "foreign-key enforcement is required"
        );
        connection.set_prepared_statement_cache_capacity(32);
        Ok(Self { connection })
    }
    pub fn catalog(&self) -> Result<Catalog> {
        let mut actors = BTreeMap::new();
        let mut query = self
            .connection
            .prepare("SELECT id,presence FROM actors ORDER BY id")?;
        for row in query.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
            let (id, state) = row?;
            actors.insert(id, Presence::parse(&state)?);
        }
        ensure!(actors.len() <= MAX_ACTORS, "actor capacity exceeded");
        let mut members: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut query = self
            .connection
            .prepare("SELECT group_id,actor_id FROM members ORDER BY group_id,actor_id")?;
        for row in query.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
            let (group, actor) = row?;
            members.entry(group).or_default().insert(actor);
        }
        ensure!(
            members.values().map(BTreeSet::len).sum::<usize>() <= MAX_MEMBERSHIPS,
            "membership capacity exceeded"
        );
        let mut groups = BTreeMap::new();
        let mut query=self.connection.prepare("SELECT id,title,topic,private,archived,pinned,revision,created_ms,last_sequence FROM groups ORDER BY id")?;
        for group in query.query_map([], |r| {
            Ok(Group {
                id: r.get(0)?,
                title: r.get(1)?,
                topic: r.get(2)?,
                private: r.get(3)?,
                archived: r.get(4)?,
                pinned: r.get(5)?,
                revision: r.get(6)?,
                created_ms: r.get(7)?,
                last_sequence: r.get(8)?,
                member_count: 0,
            })
        })? {
            let mut group = group?;
            group.member_count = members.get(&group.id).map_or(0, BTreeSet::len);
            ensure!(
                group.member_count <= MAX_MEMBERS,
                "member capacity exceeded"
            );
            groups.insert(group.id.clone(), group);
        }
        ensure!(groups.len() <= MAX_GROUPS, "group capacity exceeded");
        let mut grants = BTreeMap::new();
        let mut query = self
            .connection
            .prepare("SELECT token_hash,actor_id,expires_ms FROM grants WHERE expires_ms>?1")?;
        for row in query.query_map([now_ms()], |r| {
            Ok((
                r.get::<_, String>(0)?,
                (r.get::<_, String>(1)?, r.get::<_, u64>(2)?),
            ))
        })? {
            let (key, value) = row?;
            grants.insert(key, value);
        }
        let messages = self
            .connection
            .query_row("SELECT COUNT(*) FROM messages", [], |r| r.get(0))?;
        Ok(Catalog {
            actors,
            groups,
            members,
            grants,
            messages,
        })
    }
    pub fn register(&mut self, actors: &[String]) -> Result<()> {
        ensure!(actors.len() <= MAX_ACTORS, "actor batch exceeds bound");
        for actor in actors {
            id(actor)?;
        }
        let tx = self.connection.transaction()?;
        for actor in actors {
            tx.execute("INSERT OR IGNORE INTO actors(id) VALUES(?1)", [actor])?;
        }
        ensure!(
            tx.query_row("SELECT COUNT(*) FROM actors", [], |r| r.get::<_, usize>(0))?
                <= MAX_ACTORS,
            "actor capacity exhausted"
        );
        tx.commit()?;
        Ok(())
    }
    pub fn create_group(&mut self, group: &NewGroup) -> Result<Group> {
        group.validate()?;
        let tx = self.connection.transaction()?;
        ensure!(
            tx.query_row("SELECT COUNT(*) FROM groups", [], |r| r.get::<_, usize>(0))? < MAX_GROUPS,
            "group capacity exhausted"
        );
        ensure!(
            tx.query_row("SELECT COUNT(*) FROM members", [], |r| r.get::<_, usize>(0))?
                + group.members.len()
                <= MAX_MEMBERSHIPS,
            "membership capacity exhausted"
        );
        let created = now_ms();
        tx.execute(
            "INSERT INTO groups(id,title,topic,private,created_ms) VALUES(?1,?2,?3,?4,?5)",
            params![group.id, group.title, group.topic, group.private, created],
        )?;
        for actor in &group.members {
            tx.execute(
                "INSERT INTO members(group_id,actor_id) VALUES(?1,?2)",
                params![group.id, actor],
            )?;
        }
        tx.commit()?;
        Ok(Group {
            id: group.id.clone(),
            title: group.title.clone(),
            topic: group.topic.clone(),
            private: group.private,
            archived: false,
            pinned: false,
            revision: 1,
            created_ms: created,
            last_sequence: 0,
            member_count: group.members.len(),
        })
    }
    pub fn update_group(&mut self, change: &GroupChange) -> Result<()> {
        id(&change.id)?;
        text(&change.title, 256)?;
        text(&change.topic, 2048)?;
        let updated=self.connection.execute("UPDATE groups SET title=?1,topic=?2,archived=?3,pinned=?4,revision=revision+1 WHERE id=?5 AND revision=?6",params![change.title,change.topic,change.archived,change.pinned,change.id,change.revision])?;
        ensure!(updated == 1, "group changed; refresh before editing");
        Ok(())
    }
    pub fn membership(&mut self, group: &str, actor: &str, add: bool) -> Result<()> {
        id(group)?;
        id(actor)?;
        let tx = self.connection.transaction()?;
        let private: bool =
            tx.query_row("SELECT private FROM groups WHERE id=?1", [group], |r| {
                r.get(0)
            })?;
        if add {
            let exists: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM members WHERE group_id=?1 AND actor_id=?2)",
                params![group, actor],
                |r| r.get(0),
            )?;
            ensure!(
                !private || exists,
                "private conversation audience is immutable; create a new conversation"
            );
            ensure!(
                exists
                    || tx.query_row(
                        "SELECT COUNT(*) FROM members WHERE group_id=?1",
                        [group],
                        |r| r.get::<_, usize>(0)
                    )? < MAX_MEMBERS,
                "group member capacity exhausted"
            );
            ensure!(
                exists
                    || tx
                        .query_row("SELECT COUNT(*) FROM members", [], |r| r.get::<_, usize>(0))?
                        < MAX_MEMBERSHIPS,
                "membership capacity exhausted"
            );
            tx.execute(
                "INSERT OR IGNORE INTO members(group_id,actor_id) VALUES(?1,?2)",
                params![group, actor],
            )?;
        } else {
            tx.execute(
                "DELETE FROM members WHERE group_id=?1 AND actor_id=?2",
                params![group, actor],
            )?;
        }
        tx.execute("UPDATE groups SET revision=revision+1 WHERE id=?1", [group])?;
        tx.commit()?;
        Ok(())
    }
    pub fn set_presence(&mut self, actor: &str, state: Presence) -> Result<()> {
        ensure!(
            self.connection.execute(
                "UPDATE actors SET presence=?1 WHERE id=?2",
                params![state.code(), actor]
            )? == 1,
            "unknown digital person"
        );
        Ok(())
    }
    pub fn grant(&mut self, actor: &str, hash: &str, expires: u64) -> Result<()> {
        let tx = self.connection.transaction()?;
        tx.execute(
            "DELETE FROM grants WHERE actor_id=?1 OR expires_ms<=?2",
            params![actor, now_ms()],
        )?;
        tx.execute(
            "INSERT INTO grants(token_hash,actor_id,expires_ms) VALUES(?1,?2,?3)",
            params![hash, actor, expires],
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn history(&self, group: &str, after: u64, limit: usize) -> Result<Vec<Message>> {
        ensure!(
            limit > 0 && limit <= PAGE_SIZE,
            "history page exceeds bound"
        );
        let mut q=self.connection.prepare_cached("SELECT sequence,group_id,sender_id,request_id,text,reply_to,accepted_ms FROM messages WHERE group_id=?1 AND sequence>?2 ORDER BY sequence LIMIT ?3")?;
        let rows = q.query_map(params![group, after, limit], message_row)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
    pub fn history_before(&self, group: &str, before: u64, limit: usize) -> Result<Vec<Message>> {
        ensure!(
            limit > 0 && limit <= PAGE_SIZE,
            "history page exceeds bound"
        );
        let before = before.min(i64::MAX as u64);
        let mut q=self.connection.prepare_cached("SELECT sequence,group_id,sender_id,request_id,text,reply_to,accepted_ms FROM messages WHERE group_id=?1 AND sequence<?2 ORDER BY sequence DESC LIMIT ?3")?;
        let mut rows = q
            .query_map(params![group, before, limit], message_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.reverse();
        Ok(rows)
    }
    pub fn acknowledge(&mut self, actor: &str, group: &str, sequence: u64) -> Result<()> {
        let max: u64 = self.connection.query_row(
            "SELECT last_sequence FROM groups WHERE id=?1",
            [group],
            |r| r.get(0),
        )?;
        ensure!(sequence <= max, "acknowledgement exceeds group history");
        ensure!(
            self.connection.execute(
                "UPDATE members SET cursor=MAX(cursor,?1) WHERE group_id=?2 AND actor_id=?3",
                params![sequence, group, actor]
            )? == 1,
            "not a group member"
        );
        Ok(())
    }
    pub fn cursor(&self, actor: &str, group: &str) -> Result<u64> {
        Ok(self.connection.query_row(
            "SELECT cursor FROM members WHERE group_id=?1 AND actor_id=?2",
            params![group, actor],
            |r| r.get(0),
        )?)
    }
}
fn message_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Message> {
    Ok(Message {
        sequence: row.get(0)?,
        group_id: row.get(1)?,
        sender_id: row.get(2)?,
        request_id: row.get(3)?,
        text: row.get(4)?,
        reply_to: row.get(5)?,
        accepted_ms: row.get(6)?,
    })
}
/// Only the writer calls this within its bounded commit batch.
pub(crate) fn append(
    tx: &Transaction<'_>,
    actor: &str,
    input: &Publish,
    remaining: &mut u64,
) -> Result<Receipt> {
    input.validate()?;
    let member: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM members WHERE group_id=?1 AND actor_id=?2)",
        params![input.group_id, actor],
        |r| r.get(0),
    )?;
    ensure!(
        member,
        "digital person is not a member of this conversation"
    );
    let prior=tx.query_row("SELECT sequence,group_id,sender_id,request_id,text,reply_to,accepted_ms FROM messages WHERE group_id=?1 AND sender_id=?2 AND request_id=?3",params![input.group_id,actor,input.request_id],message_row).optional()?;
    if let Some(prior) = prior {
        ensure!(
            prior.text == input.text && prior.reply_to == input.reply_to,
            "request ID already binds different content"
        );
        return Ok(Receipt {
            message: prior,
            duplicate: true,
            durable: true,
        });
    }
    ensure!(
        *remaining > 0,
        "durable message capacity exhausted; archive policy requires explicit maintenance"
    );
    let archived: bool = tx.query_row(
        "SELECT archived FROM groups WHERE id=?1",
        [&input.group_id],
        |r| r.get(0),
    )?;
    ensure!(!archived, "conversation is archived");
    if let Some(parent) = input.reply_to {
        ensure!(
            tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM messages WHERE sequence=?1 AND group_id=?2)",
                params![parent, input.group_id],
                |r| r.get::<_, bool>(0)
            )?,
            "reply belongs to another conversation or is unavailable"
        );
    }
    let at = now_ms();
    tx.execute("INSERT INTO messages(group_id,sender_id,request_id,text,reply_to,accepted_ms) VALUES(?1,?2,?3,?4,?5,?6)",params![input.group_id,actor,input.request_id,input.text,input.reply_to,at])?;
    let sequence = tx.last_insert_rowid() as u64;
    tx.execute(
        "UPDATE groups SET last_sequence=?1 WHERE id=?2",
        params![sequence, input.group_id],
    )?;
    *remaining -= 1;
    Ok(Receipt {
        message: Message {
            sequence,
            group_id: input.group_id.clone(),
            sender_id: actor.into(),
            request_id: input.request_id.clone(),
            text: input.text.clone(),
            reply_to: input.reply_to,
            accepted_ms: at,
        },
        duplicate: false,
        durable: true,
    })
}
