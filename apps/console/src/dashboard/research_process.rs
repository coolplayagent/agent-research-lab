//! Host projection fenced to a current collaboration assignment.
use super::*;
use collaboration_model::{Goal, WorkItem};

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Scope {
    pub group_id: String,
    pub goal_id: String,
    pub work_id: String,
    pub attempt: u32,
    pub before: Option<u64>,
}
fn assignment<'a>(scope: &Scope, goal: &'a Goal) -> Result<&'a WorkItem> {
    ensure!(
        goal.input.group_id == scope.group_id
            && goal.input.id == scope.goal_id
            && goal.input.scenario == "research",
        "research scope mismatch"
    );
    let work = goal
        .work
        .iter()
        .find(|work| work.id == scope.work_id)
        .context("unknown assignment")?;
    ensure!(
        work.attempt == scope.attempt && scope.attempt > 0,
        "stale attempt"
    );
    Ok(work)
}
fn validate_job(job: &Job, id: &str, person: &str) -> Result<()> {
    ensure!(
        job.task.id == id
            && job.task.persona_id.as_deref() == Some(person)
            && job
                .persona
                .as_ref()
                .is_some_and(|persona| persona.id == person)
            && job.attempt == 1
            && job.run_id == format!("{id}-attempt-1"),
        "execution fence mismatch"
    );
    Ok(())
}
fn cursor_before(id: &str) -> String {
    let mut bytes = id.as_bytes().to_vec();
    *bytes.last_mut().unwrap() -= 1;
    if bytes.len() < 100 {
        bytes.push(b'~');
    }
    String::from_utf8(bytes).unwrap()
}
async fn find(web: &transport::Web, scope: &Scope) -> Result<Goal> {
    let mut after = cursor_before(&scope.goal_id);
    for _ in 0..16 {
        let page = web.hub.goals(&scope.group_id, "", &after, false).await?;
        let goals: Vec<Goal> = serde_json::from_value(page["goals"].clone())?;
        if let Some(goal) = goals
            .into_iter()
            .find(|goal| goal.input.id == scope.goal_id)
        {
            assignment(scope, &goal)?;
            return Ok(goal);
        }
        let Some(next) = page["next_after"].as_str() else {
            break;
        };
        ensure!(
            next > after.as_str() && next < scope.goal_id.as_str(),
            "goal unavailable"
        );
        after = next.into();
    }
    anyhow::bail!("research assignment unavailable")
}
pub(super) async fn get(web: &transport::Web, scope: Scope) -> Result<Value> {
    for id in [&scope.group_id, &scope.goal_id, &scope.work_id] {
        collaboration_model::id(id)?;
    }
    let goal = find(web, &scope).await?;
    let work = assignment(&scope, &goal)?;
    let id = research_scenario::job_id(&goal, work);
    let person = work.person_id.clone();
    let config = web.config.clone().context("research unavailable")?;
    let before = scope.before;
    let mut value = tokio::task::spawn_blocking(move || {
        let job = read_job(&config.state_dir.join("jobs").join(format!("{id}.json")))?;
        validate_job(&job, &id, &person)?;
        sessions::page(&config.state_dir, &job.run_id, before)
    })
    .await??;
    let current = find(web, &scope).await?;
    let current_work = assignment(&scope, &current)?;
    ensure!(
        current.revision == goal.revision && current_work.claim_id == work.claim_id,
        "research assignment changed during read"
    );
    value["group_id"] = json!(scope.group_id);
    value["goal_id"] = json!(scope.goal_id);
    value["goal_revision"] = json!(goal.revision);
    value["work_id"] = json!(scope.work_id);
    value["attempt"] = json!(scope.attempt);
    Ok(value)
}

#[cfg(test)]
#[path = "research_process_tests.rs"]
mod tests;
