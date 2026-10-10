use crate::{goals::*, *};
use rusqlite::params;
impl Store {
    pub(crate) fn goal_work(
        &mut self,
        actor: &str,
        credential: Option<&str>,
        change: WorkChange,
    ) -> Result<Value> {
        let (goal_id, work_id, claim_id, attempt) = change.ids();
        for value in [goal_id, work_id, claim_id] {
            id(value)?;
        }
        let tx = self.connection.transaction()?;
        if let Some(hash) = credential {
            ensure!(tx.query_row("SELECT EXISTS(SELECT 1 FROM grants WHERE token_hash=?1 AND actor_id=?2 AND expires_ms>?3)", params![hash, actor, now_ms()], |r| r.get::<_, bool>(0))?, "credential revoked or expired before commit");
        }
        let mut goal = read(&tx, goal_id)?;
        ensure!(goal.state == GoalState::Active, "goal is closed");
        available(&tx, &goal.input.group_id)?;
        member(&tx, &goal.input.group_id, actor)?;
        // Research output can only enter through its host adapter and existing evidence gates.
        ensure!(
            credential.is_none() || goal.input.scenario == "collaboration",
            "scenario tasks are owned by the host adapter"
        );
        let work = goal
            .work
            .iter_mut()
            .find(|w| w.id == work_id && w.person_id == actor)
            .context("assignment belongs to another Agent")?;
        ensure!(work.attempt == attempt, "attempt has been superseded");
        let note = match &change {
            WorkChange::Claim { .. } => {
                if work.claim_id.as_deref() == Some(claim_id) && work.state == WorkState::Running {
                    ensure!(
                        work.deadline_ms.is_some_and(|d| d > now_ms()),
                        "execution lease expired"
                    );
                    return Ok(json!(goal));
                }
                ensure!(
                    work.state == WorkState::Ready,
                    "assignment already claimed or finished"
                );
                work.state = WorkState::Running;
                work.claim_id = Some(claim_id.into());
                work.deadline_ms = Some(now_ms() + goal.input.max_seconds * 1000);
                format!("{actor} 已领取任务")
            }
            WorkChange::Submit { result, .. } => {
                text(&result.summary, 8000)?;
                ensure!(
                    !result.summary.trim().is_empty()
                        && serde_json::to_vec(&result.evidence)?.len() <= 16000,
                    "result needs a summary and bounded evidence"
                );
                ensure!(
                    work.claim_id.as_deref() == Some(claim_id),
                    "claim does not match the current attempt"
                );
                if work.result.as_ref() == Some(result) {
                    return Ok(json!(goal));
                }
                ensure!(
                    work.state == WorkState::Running
                        && work.deadline_ms.is_some_and(|d| d > now_ms()),
                    "assignment lease expired or already finished"
                );
                work.result = Some(result.clone());
                work.state = if result.succeeded {
                    WorkState::Submitted
                } else {
                    WorkState::Failed
                };
                format!(
                    "{actor} {}：{}",
                    if result.succeeded {
                        "已提交结果，等待验收"
                    } else {
                        "任务失败"
                    },
                    result.summary
                )
            }
        };
        goal.revision += 1;
        save(&tx, &goal, &note)?;
        tx.commit()?;
        Ok(json!(goal))
    }
}
