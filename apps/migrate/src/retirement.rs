//! Offline erasure scope for the removed collaboration evolution features.
use anyhow::{Result, ensure};
use rusqlite::Connection;
fn exists(db: &Connection, table: &str) -> Result<bool> {
    Ok(db.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name=?1)",
        [table],
        |r| r.get(0),
    )?)
}
pub(crate) fn prepare(db: &Connection) -> Result<()> {
    db.execute_batch(
        "CREATE TEMP TABLE retired_goals(id TEXT PRIMARY KEY);
        CREATE TEMP TABLE retired_outcomes(id TEXT PRIMARY KEY);
        CREATE TEMP TABLE retired_messages(sequence INTEGER PRIMARY KEY);",
    )?;
    if exists(db, "crystal_meta")?
        && db.query_row(
            "SELECT value FROM crystal_meta WHERE key='schema'",
            [],
            |r| r.get::<_, u64>(0),
        )? >= 10
    {
        return Ok(());
    }
    if exists(db, "evolution_goal_bindings")? {
        db.execute_batch("INSERT INTO retired_goals SELECT goal_id FROM evolution_goal_bindings;")?;
    }
    if exists(db, "goals")? {
        db.execute_batch("INSERT OR IGNORE INTO retired_goals SELECT id FROM goals WHERE scenario='software-evolution';")?;
    }
    if exists(db, "research_topics")? {
        db.execute_batch("INSERT OR IGNORE INTO retired_goals SELECT id FROM goals WHERE group_id IN (SELECT group_id FROM research_topics);")?;
    }
    if exists(db, "outcome_paths")? {
        db.execute_batch("INSERT OR IGNORE INTO retired_goals SELECT goal_id FROM outcome_paths;")?;
    }
    if exists(db, "outcomes")? {
        db.execute_batch("INSERT OR IGNORE INTO retired_goals SELECT goal_id FROM outcomes;")?;
    }
    if db.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('messages') WHERE name='event')",
        [],
        |r| r.get::<_, bool>(0),
    )? {
        db.execute_batch("INSERT INTO retired_messages SELECT sequence FROM messages WHERE json_extract(event,'$.goal_id') IN (SELECT id FROM retired_goals);")?;
    }
    if exists(db, "outcomes")? {
        db.execute_batch("INSERT INTO retired_outcomes SELECT id FROM outcomes WHERE goal_id IN (SELECT id FROM retired_goals);")?;
        ensure!(!db.query_row("SELECT EXISTS(SELECT 1 FROM outcome_references WHERE parent_id IN (SELECT id FROM retired_outcomes) AND child_id NOT IN (SELECT id FROM retired_outcomes))", [], |r| r.get::<_, bool>(0))?, "retirement blocked: an independent outcome references iteration evidence");
    }
    Ok(())
}
/// Exclude only explicitly retired rows from the migration preservation digest.
pub(crate) fn predicate(table: &str) -> &'static str {
    match table {
        "goals" => "id NOT IN (SELECT id FROM retired_goals)",
        "goal_assignees" | "outcome_paths" => "goal_id NOT IN (SELECT id FROM retired_goals)",
        "outcomes" => "id NOT IN (SELECT id FROM retired_outcomes)",
        "outcome_events" => "outcome_id NOT IN (SELECT id FROM retired_outcomes)",
        "outcome_references" => {
            "child_id NOT IN (SELECT id FROM retired_outcomes) AND parent_id NOT IN (SELECT id FROM retired_outcomes)"
        }
        _ => "1",
    }
}
pub(crate) const ERASE: &str = "
    UPDATE messages SET text='',event=NULL WHERE sequence IN (SELECT sequence FROM retired_messages);
    DELETE FROM outcome_references WHERE child_id IN (SELECT id FROM retired_outcomes) OR parent_id IN (SELECT id FROM retired_outcomes);
    DELETE FROM outcome_events WHERE outcome_id IN (SELECT id FROM retired_outcomes);
    DELETE FROM outcomes WHERE id IN (SELECT id FROM retired_outcomes);
    DELETE FROM outcome_paths WHERE goal_id IN (SELECT id FROM retired_goals);
    DELETE FROM goal_assignees WHERE goal_id IN (SELECT id FROM retired_goals);
    DELETE FROM goals WHERE id IN (SELECT id FROM retired_goals);";

#[cfg(test)]
#[path = "retirement_tests.rs"]
mod tests;
