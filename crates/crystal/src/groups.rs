//! Group lifecycle and membership mutations are committed atomically.
use crate::{model::*, store::Store};
use anyhow::{Result, ensure};
use rusqlite::params;
impl Store {
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
            "INSERT INTO groups(id,title,topic,private,created_ms,kind) VALUES(?1,?2,?3,?4,?5,?6)",
            params![
                group.id,
                group.title,
                group.topic,
                group.private,
                created,
                group.kind.code()
            ],
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
            kind: group.kind,
            archived: false,
            pinned: false,
            revision: 1,
            created_ms: created,
            last_sequence: 0,
            member_count: group.members.len(),
        })
    }
    pub fn update_group(&mut self, change: &GroupChange) -> Result<()> {
        if change.archived {
            ensure!(
                !self.connection.query_row(
                    "SELECT EXISTS(SELECT 1 FROM goals WHERE group_id=?1 AND active=1)",
                    [&change.id],
                    |r| r.get::<_, bool>(0)
                )?,
                "finish or cancel active goals before archiving"
            );
        }
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
        let archived: bool =
            tx.query_row("SELECT archived FROM groups WHERE id=?1", [group], |r| {
                r.get(0)
            })?;
        ensure!(!archived, "conversation is archived");
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
            ensure!(!tx.query_row("SELECT EXISTS(SELECT 1 FROM goals g WHERE g.group_id=?1 AND g.active=1 AND (?2='operator' OR EXISTS(SELECT 1 FROM goal_assignees a WHERE a.goal_id=g.id AND a.actor_id=?2)))", params![group,actor], |r| r.get::<_, bool>(0))?, "finish or cancel the member's active goals before removal");
            tx.execute(
                "DELETE FROM members WHERE group_id=?1 AND actor_id=?2",
                params![group, actor],
            )?;
        }
        tx.execute("UPDATE groups SET revision=revision+1 WHERE id=?1", [group])?;
        tx.commit()?;
        Ok(())
    }
}
