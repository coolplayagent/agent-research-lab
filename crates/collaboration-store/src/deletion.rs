//! Deletion runs inside the single storage writer, with revision and evidence guards.
use crate::*;
use contracts::DeletionCode;
use rusqlite::{Connection, OptionalExtension, params};
impl Hub {
    pub async fn delete(&self, target: DeleteTarget) -> Result<DeletionResult> {
        Ok(serde_json::from_value(
            self.control(Control::Delete(target)).await?,
        )?)
    }
}
fn result(code: DeletionCode, references: Vec<String>) -> DeletionResult {
    DeletionResult { code, references }
}
fn ids(db: &Connection, sql: &str, id: &str) -> Result<Vec<String>> {
    Ok(db
        .prepare(sql)?
        .query_map([id], |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?)
}
impl Store {
    pub(crate) fn delete(&mut self, target: &DeleteTarget) -> Result<DeletionResult> {
        let tx = self.connection.transaction()?;
        let (id, revision, table) = match target {
            DeleteTarget::Group { id, revision } => (id, revision, "groups"),
            DeleteTarget::Announcement {
                group_id, revision, ..
            } => (group_id, revision, "groups"),
        };
        collaboration_model::id(id)?;
        let current: Option<u64> = tx
            .query_row(
                &format!("SELECT revision FROM {table} WHERE id=?1"),
                [id],
                |row| row.get(0),
            )
            .optional()?;
        if current.is_none() {
            return Ok(result(DeletionCode::NotFound, vec![]));
        }
        if current != Some(*revision) {
            return Ok(result(DeletionCode::RevisionConflict, vec![]));
        }
        match target {
            DeleteTarget::Group { .. } => {
                let kind: String =
                    tx.query_row("SELECT kind FROM groups WHERE id=?1", [id], |r| r.get(0))?;
                if GroupKind::parse(&kind)? == GroupKind::Board {
                    return Ok(result(DeletionCode::Unsupported, vec![]));
                }
                let active = ids(
                    &tx,
                    "SELECT id FROM goals WHERE group_id=?1 AND active=1 LIMIT 100",
                    id,
                )?;
                if !active.is_empty() {
                    return Ok(result(DeletionCode::ActiveGoals, active));
                }
                let mut refs = ids(
                    &tx,
                    "SELECT id FROM outcomes WHERE group_id=?1 UNION SELECT id FROM outcome_tasks WHERE group_id=?1 LIMIT 100",
                    id,
                )?;
                let historical = tx.prepare("SELECT DISTINCT json_extract(event,'$.goal_id') FROM messages WHERE group_id=?1 AND json_extract(event,'$.kind') IN (?2,?3) LIMIT 100")?.query_map(params![id, contracts::GoalEventKind::Submitted.as_str(), contracts::GoalEventKind::Failed.as_str()], |row| row.get::<_, String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
                refs.extend(historical);
                for value in ids(&tx, "SELECT value FROM goals WHERE group_id=?1", id)? {
                    let goal: Goal = serde_json::from_str(&value)?;
                    if goal.work.iter().any(|work| work.result.is_some()) {
                        refs.push(goal.input.id);
                    }
                }
                if !refs.is_empty() {
                    refs.truncate(100);
                    return Ok(result(DeletionCode::EvidenceReferences, refs));
                }
                tx.execute("DELETE FROM goal_assignees WHERE goal_id IN (SELECT id FROM goals WHERE group_id=?1)", [id])?;
                tx.execute("DELETE FROM goals WHERE group_id=?1", [id])?;
                tx.execute("DELETE FROM messages WHERE group_id=?1", [id])?;
                tx.execute("DELETE FROM members WHERE group_id=?1", [id])?;
                tx.execute("DELETE FROM groups WHERE id=?1", [id])?;
            }
            DeleteTarget::Announcement { sequence, .. } => {
                let kind: String =
                    tx.query_row("SELECT kind FROM groups WHERE id=?1", [id], |r| r.get(0))?;
                if GroupKind::parse(&kind)? != GroupKind::Board {
                    return Ok(result(DeletionCode::Unsupported, vec![]));
                }
                let event: Option<Option<String>> = tx
                    .query_row(
                        "SELECT event FROM messages WHERE group_id=?1 AND sequence=?2",
                        params![id, sequence],
                        |r| r.get(0),
                    )
                    .optional()?;
                match event {
                    None => return Ok(result(DeletionCode::NotFound, vec![])),
                    Some(Some(_)) => return Ok(result(DeletionCode::Unsupported, vec![])),
                    Some(None) => {}
                }
                // Empty text is reserved for tombstones: normal publishing rejects it.
                tx.execute(
                    "UPDATE messages SET text='' WHERE group_id=?1 AND sequence=?2",
                    params![id, sequence],
                )?;
                tx.execute("UPDATE groups SET revision=revision+1 WHERE id=?1", [id])?;
            }
        }
        tx.commit()?;
        Ok(result(DeletionCode::Deleted, vec![]))
    }
}
#[cfg(test)]
#[path = "deletion_tests.rs"]
mod tests;
