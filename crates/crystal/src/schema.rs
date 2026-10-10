//! Additive migration preserves all v1 conversations, identities and replay cursors.
use anyhow::{Result, ensure};
use rusqlite::Connection;
pub(crate) fn migrate(connection: &Connection) -> Result<()> {
    let version: u64 = connection.query_row(
        "SELECT value FROM crystal_meta WHERE key='schema'",
        [],
        |r| r.get(0),
    )?;
    ensure!((1..=3).contains(&version), "unsupported crystal schema");
    if version == 1 {
        connection.execute_batch(
            "BEGIN IMMEDIATE;
            ALTER TABLE actors ADD COLUMN name TEXT NOT NULL DEFAULT '';
            ALTER TABLE actors ADD COLUMN application_id TEXT;
            UPDATE actors SET name=id;
            ALTER TABLE groups ADD COLUMN kind TEXT NOT NULL DEFAULT 'conversation';
            UPDATE crystal_meta SET value=2 WHERE key='schema';
            COMMIT;",
        )?;
    }
    if version < 3 {
        connection.execute_batch("BEGIN IMMEDIATE;
            CREATE TABLE goals(id TEXT PRIMARY KEY, group_id TEXT NOT NULL REFERENCES groups(id), scenario TEXT NOT NULL, active INTEGER NOT NULL, value TEXT NOT NULL);
            CREATE INDEX goals_group ON goals(group_id,id);
            CREATE INDEX goals_scenario ON goals(scenario,active,id);
            CREATE TABLE goal_assignees(goal_id TEXT NOT NULL REFERENCES goals(id), actor_id TEXT NOT NULL REFERENCES actors(id), PRIMARY KEY(goal_id,actor_id));
            CREATE INDEX goal_actor ON goal_assignees(actor_id,goal_id);
            UPDATE crystal_meta SET value=3 WHERE key='schema'; COMMIT;")?;
    }
    Ok(())
}
