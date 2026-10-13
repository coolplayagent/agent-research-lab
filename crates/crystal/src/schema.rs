//! Additive migration preserves all v1 conversations, identities and replay cursors.
use anyhow::{Result, ensure};
use rusqlite::Connection;
pub(crate) fn migrate(connection: &Connection) -> Result<()> {
    let version: u64 = connection.query_row(
        "SELECT value FROM crystal_meta WHERE key='schema'",
        [],
        |r| r.get(0),
    )?;
    ensure!((1..=6).contains(&version), "unsupported crystal schema");
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
    if version < 4 {
        connection.execute_batch(
            "BEGIN IMMEDIATE;
            ALTER TABLE messages ADD COLUMN event TEXT;
            UPDATE crystal_meta SET value=4 WHERE key='schema'; COMMIT;",
        )?;
    }
    if version < 5 {
        connection.execute_batch("BEGIN IMMEDIATE;
          CREATE TABLE research_topics(id TEXT PRIMARY KEY,group_id TEXT NOT NULL REFERENCES groups(id),revision INTEGER NOT NULL,value TEXT NOT NULL);
          CREATE TABLE research_subjects(id TEXT PRIMARY KEY,topic_id TEXT NOT NULL REFERENCES research_topics(id),revision INTEGER NOT NULL,value TEXT NOT NULL);
          CREATE INDEX subjects_topic ON research_subjects(topic_id,id);
          CREATE TABLE research_nodes(sequence INTEGER PRIMARY KEY AUTOINCREMENT,id TEXT NOT NULL UNIQUE,subject_id TEXT NOT NULL REFERENCES research_subjects(id),goal_id TEXT REFERENCES goals(id),value TEXT NOT NULL);
          CREATE INDEX nodes_subject ON research_nodes(subject_id,sequence);
          CREATE TABLE research_edges(child_id TEXT NOT NULL REFERENCES research_nodes(id),parent_id TEXT NOT NULL REFERENCES research_nodes(id),PRIMARY KEY(child_id,parent_id));
          UPDATE crystal_meta SET value=5 WHERE key='schema'; COMMIT;")?;
    }
    if version < 6 {
        connection.execute_batch("BEGIN IMMEDIATE;
          CREATE TABLE evolution_runs(id TEXT PRIMARY KEY,group_id TEXT NOT NULL REFERENCES groups(id),scenario TEXT NOT NULL,active INTEGER NOT NULL,value TEXT NOT NULL);
          CREATE INDEX evolution_runs_group ON evolution_runs(group_id,id);
          CREATE TABLE evolution_goal_bindings(goal_id TEXT PRIMARY KEY REFERENCES goals(id),run_id TEXT NOT NULL REFERENCES evolution_runs(id),round INTEGER NOT NULL);
          CREATE TABLE evolution_requests(run_id TEXT NOT NULL REFERENCES evolution_runs(id),request_id TEXT NOT NULL,digest TEXT NOT NULL,PRIMARY KEY(run_id,request_id));
          CREATE TABLE research_assessments(sequence INTEGER PRIMARY KEY AUTOINCREMENT,id TEXT NOT NULL UNIQUE,node_id TEXT NOT NULL REFERENCES research_nodes(id),goal_id TEXT NOT NULL REFERENCES goals(id),value TEXT NOT NULL);
          CREATE INDEX assessments_node ON research_assessments(node_id,sequence);
          UPDATE crystal_meta SET value=6 WHERE key='schema'; COMMIT;")?;
    }
    Ok(())
}
