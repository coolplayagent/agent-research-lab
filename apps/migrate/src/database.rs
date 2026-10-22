//! Offline upgrades for historical collaboration databases.
use anyhow::{Result, ensure};
use rusqlite::Connection;
const STEPS: &[(u64, &str)] = &[
    (2, "BEGIN IMMEDIATE;
            ALTER TABLE actors ADD COLUMN name TEXT NOT NULL DEFAULT '';
            ALTER TABLE actors ADD COLUMN application_id TEXT;
            UPDATE actors SET name=id;
            ALTER TABLE groups ADD COLUMN kind TEXT NOT NULL DEFAULT 'conversation';
            UPDATE crystal_meta SET value=2 WHERE key='schema';
            COMMIT;"),
    (3, "BEGIN IMMEDIATE;
            CREATE TABLE goals(id TEXT PRIMARY KEY, group_id TEXT NOT NULL REFERENCES groups(id), scenario TEXT NOT NULL, active INTEGER NOT NULL, value TEXT NOT NULL);
            CREATE INDEX goals_group ON goals(group_id,id);
            CREATE INDEX goals_scenario ON goals(scenario,active,id);
            CREATE TABLE goal_assignees(goal_id TEXT NOT NULL REFERENCES goals(id), actor_id TEXT NOT NULL REFERENCES actors(id), PRIMARY KEY(goal_id,actor_id));
            CREATE INDEX goal_actor ON goal_assignees(actor_id,goal_id);
            UPDATE crystal_meta SET value=3 WHERE key='schema'; COMMIT;"),
    (4, "BEGIN IMMEDIATE;
            ALTER TABLE messages ADD COLUMN event TEXT;
            UPDATE crystal_meta SET value=4 WHERE key='schema'; COMMIT;"),
    (5, "BEGIN IMMEDIATE;
          CREATE TABLE research_topics(id TEXT PRIMARY KEY,group_id TEXT NOT NULL REFERENCES groups(id),revision INTEGER NOT NULL,value TEXT NOT NULL);
          CREATE TABLE research_subjects(id TEXT PRIMARY KEY,topic_id TEXT NOT NULL REFERENCES research_topics(id),revision INTEGER NOT NULL,value TEXT NOT NULL);
          CREATE INDEX subjects_topic ON research_subjects(topic_id,id);
          CREATE TABLE research_nodes(sequence INTEGER PRIMARY KEY AUTOINCREMENT,id TEXT NOT NULL UNIQUE,subject_id TEXT NOT NULL REFERENCES research_subjects(id),goal_id TEXT REFERENCES goals(id),value TEXT NOT NULL);
          CREATE INDEX nodes_subject ON research_nodes(subject_id,sequence);
          CREATE TABLE research_edges(child_id TEXT NOT NULL REFERENCES research_nodes(id),parent_id TEXT NOT NULL REFERENCES research_nodes(id),PRIMARY KEY(child_id,parent_id));
          UPDATE crystal_meta SET value=5 WHERE key='schema'; COMMIT;"),
    (6, "BEGIN IMMEDIATE;
          CREATE TABLE evolution_runs(id TEXT PRIMARY KEY,group_id TEXT NOT NULL REFERENCES groups(id),scenario TEXT NOT NULL,active INTEGER NOT NULL,value TEXT NOT NULL);
          CREATE INDEX evolution_runs_group ON evolution_runs(group_id,id);
          CREATE TABLE evolution_goal_bindings(goal_id TEXT PRIMARY KEY REFERENCES goals(id),run_id TEXT NOT NULL REFERENCES evolution_runs(id),round INTEGER NOT NULL);
          CREATE TABLE evolution_requests(run_id TEXT NOT NULL REFERENCES evolution_runs(id),request_id TEXT NOT NULL,digest TEXT NOT NULL,PRIMARY KEY(run_id,request_id));
          CREATE TABLE research_assessments(sequence INTEGER PRIMARY KEY AUTOINCREMENT,id TEXT NOT NULL UNIQUE,node_id TEXT NOT NULL REFERENCES research_nodes(id),goal_id TEXT NOT NULL REFERENCES goals(id),value TEXT NOT NULL);
          CREATE INDEX assessments_node ON research_assessments(node_id,sequence);
          UPDATE crystal_meta SET value=6 WHERE key='schema'; COMMIT;"),
    (7, "BEGIN IMMEDIATE;
          CREATE TABLE outcome_paths(topic_id TEXT NOT NULL REFERENCES research_topics(id),goal_id TEXT NOT NULL REFERENCES goals(id),value TEXT NOT NULL,PRIMARY KEY(topic_id,goal_id));
          CREATE TABLE outcomes(sequence INTEGER PRIMARY KEY AUTOINCREMENT,id TEXT NOT NULL UNIQUE,topic_id TEXT NOT NULL REFERENCES research_topics(id),goal_id TEXT NOT NULL REFERENCES goals(id),author_id TEXT NOT NULL REFERENCES actors(id),kind TEXT NOT NULL,title TEXT NOT NULL,summary TEXT NOT NULL,value TEXT NOT NULL);
          CREATE INDEX outcomes_topic ON outcomes(topic_id,sequence);
          CREATE TABLE outcome_references(child_id TEXT NOT NULL REFERENCES outcomes(id),parent_id TEXT NOT NULL REFERENCES outcomes(id),kind TEXT NOT NULL,PRIMARY KEY(child_id,parent_id));
          CREATE INDEX outcome_references_parent ON outcome_references(parent_id,child_id);
          CREATE TABLE outcome_events(sequence INTEGER PRIMARY KEY AUTOINCREMENT,outcome_id TEXT NOT NULL REFERENCES outcomes(id),kind TEXT NOT NULL,request_id TEXT NOT NULL,value TEXT NOT NULL,created_ms INTEGER NOT NULL,UNIQUE(kind,request_id));
          CREATE INDEX outcome_events_owner ON outcome_events(outcome_id,sequence);
          UPDATE crystal_meta SET value=7 WHERE key='schema'; COMMIT;"),
    (8, "BEGIN IMMEDIATE;
          ALTER TABLE actors ADD COLUMN archived INTEGER NOT NULL DEFAULT 0;
          UPDATE crystal_meta SET value=8 WHERE key='schema'; COMMIT;"),
];
fn upgrade(connection: &Connection) -> Result<()> {
    let version: u64 = connection.query_row(
        "SELECT value FROM crystal_meta WHERE key='schema'",
        [],
        |r| r.get(0),
    )?;
    ensure!(
        (1..=8).contains(&version),
        "unsupported collaboration schema"
    );
    for (next, sql) in STEPS {
        if version < *next {
            connection.execute_batch(sql)?;
        }
    }
    Ok(())
}
#[cfg(test)]
pub(crate) fn fixture_version(connection: &Connection, version: u64) -> Result<()> {
    bootstrap(connection)?;
    for (next, sql) in STEPS {
        if *next <= version {
            connection.execute_batch(sql)?;
        }
    }
    Ok(())
}

fn bootstrap(connection: &Connection) -> Result<()> {
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
    Ok(())
}

use anyhow::Context;
use rusqlite::{OpenFlags, OptionalExtension};
use std::{collections::BTreeMap, path::Path};
fn columns(connection: &Connection) -> Result<BTreeMap<String, Vec<String>>> {
    let mut tables = connection.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' AND name != 'crystal_meta' ORDER BY name")?;
    let names = tables
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    names
        .into_iter()
        .map(|name| {
            let mut query = connection.prepare(&format!("PRAGMA table_info({})", quoted(&name)))?;
            let cols = query
                .query_map([], |r| r.get::<_, String>(1))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok((name, cols))
        })
        .collect()
}
fn quoted(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}
fn rows(
    connection: &Connection,
    tables: &BTreeMap<String, Vec<String>>,
) -> Result<BTreeMap<String, String>> {
    let mut result = BTreeMap::new();
    for (table, cols) in tables {
        let selection = cols.iter().map(|c| quoted(c)).collect::<Vec<_>>().join(",");
        let mut query = connection.prepare(&format!(
            "SELECT {selection} FROM {} ORDER BY {selection}",
            quoted(table)
        ))?;
        let mut cursor = query.query([])?;
        use sha2::{Digest, Sha256};
        let mut hash = Sha256::new();
        while let Some(row) = cursor.next()? {
            for i in 0..cols.len() {
                // Include storage type and length, so delimiters inside content cannot collide.
                let value = match row.get_ref(i)? {
                    rusqlite::types::ValueRef::Null => vec![0],
                    rusqlite::types::ValueRef::Integer(v) => {
                        [vec![1], v.to_le_bytes().to_vec()].concat()
                    }
                    rusqlite::types::ValueRef::Real(v) => {
                        [vec![2], v.to_bits().to_le_bytes().to_vec()].concat()
                    }
                    rusqlite::types::ValueRef::Text(v) => [vec![3], v.to_vec()].concat(),
                    rusqlite::types::ValueRef::Blob(v) => [vec![4], v.to_vec()].concat(),
                };
                hash.update((value.len() as u64).to_le_bytes());
                hash.update(value);
            }
        }
        result.insert(table.clone(), format!("{:x}", hash.finalize()));
    }
    Ok(result)
}
pub fn convert(path: &Path) -> Result<()> {
    let connection = Connection::open(path)?;
    connection.busy_timeout(std::time::Duration::from_millis(100))?;
    let has_meta: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='crystal_meta')",
        [],
        |r| r.get(0),
    )?;
    if has_meta {
        let version: u64 = connection.query_row(
            "SELECT value FROM crystal_meta WHERE key='schema'",
            [],
            |r| r.get(0),
        )?;
        ensure!(
            (1..=8).contains(&version),
            "unsupported database schema {version}"
        );
    } else {
        let tables = columns(&connection)?;
        ensure!(
            tables.keys().all(|t| matches!(
                t.as_str(),
                "actors" | "groups" | "members" | "messages" | "grants"
            )) && ["actors", "groups", "members", "messages"]
                .iter()
                .all(|t| tables.contains_key(*t)),
            "unrecognized unversioned database"
        );
    }
    let cols = columns(&connection)?;
    let before = rows(&connection, &cols)?;
    bootstrap(&connection)?;
    upgrade(&connection)?;
    ensure!(
        before == rows(&connection, &cols)?,
        "database conversion changed existing rows"
    );
    connection.execute_batch("PRAGMA wal_checkpoint(TRUNCATE); PRAGMA journal_mode=DELETE;")?;
    drop(connection);
    validate(path)
}
pub fn validate(path: &Path) -> Result<()> {
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let version: u64 = connection.query_row(
        "SELECT value FROM crystal_meta WHERE key='schema'",
        [],
        |r| r.get(0),
    )?;
    ensure!(version == 8, "target database is not current");
    let integrity: String = connection.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
    ensure!(integrity == "ok", "database integrity failed");
    ensure!(
        connection
            .query_row("PRAGMA foreign_key_check", [], |_| Ok(()))
            .optional()?
            .is_none(),
        "database references are invalid"
    );
    connection
        .prepare(
            "SELECT actors.name, actors.application_id, groups.kind FROM actors, groups LIMIT 0",
        )
        .context("incomplete current schema")?;
    for table in [
        "goals",
        "goal_assignees",
        "research_topics",
        "research_subjects",
        "research_nodes",
        "research_edges",
        "evolution_runs",
        "evolution_goal_bindings",
        "evolution_requests",
        "research_assessments",
        "outcome_paths",
        "outcomes",
        "outcome_references",
        "outcome_events",
    ] {
        connection.prepare(&format!("SELECT * FROM {table} LIMIT 0"))?;
    }
    Ok(())
}
