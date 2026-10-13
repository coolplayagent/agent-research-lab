//! Host projection fenced to a current collaboration assignment.
use super::*;
use crystal::{Goal, WorkItem};

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
            && matches!(
                goal.input.scenario.as_str(),
                "research" | "software-evolution"
            ),
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
        crystal::id(id)?;
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
mod tests {
    use super::*;
    #[tokio::test]
    async fn storage_api_scope_projects_only_the_current_person_bound_run() {
        use std::io::Write;
        let root = std::env::temp_dir().join(format!(
            "research-process-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = Cleanup(root.clone());
        let hub =
            im_storage::Client::from(crystal::Hub::open(&root.join("collaboration")).unwrap());
        hub.register_operator().await.unwrap();
        hub.save_person(crystal::Person {
            id: "person".into(),
            name: "Researcher".into(),
            application_id: Some("research".into()),
        })
        .await
        .unwrap();
        hub.create_group(crystal::NewGroup {
            id: "group".into(),
            title: "Research".into(),
            topic: "Process".into(),
            kind: crystal::GroupKind::Conversation,
            private: false,
            members: vec!["operator".into(), "person".into()],
        })
        .await
        .unwrap();
        let input: crystal::GoalInput = serde_json::from_value(json!({"id":"goal","group_id":"group","title":"Research","objective":"Inspect","acceptance":"Evidence","scenario":"research","parameters":{},"assignments":[{"person_id":"person","instruction":"Inspect"}],"max_seconds":180})).unwrap();
        let goal = hub
            .change_goal(crystal::GoalChange::Create { goal: input })
            .await
            .unwrap();
        let work = &goal.work[0];
        let id = research_scenario::job_id(&goal, work);
        let run = format!("{id}-attempt-1");
        let job: Job = serde_json::from_value(json!({"task":{"id":id,"persona_id":"person","role":"research","repository":"superpod","prompt":"PRIVATE_PROMPT"},"persona":{"id":"person","name":"Researcher","revision":1,"soul":"PRIVATE_SOUL","memory":null},"model":"fixture","source_commit":"a".repeat(40),"superpod_commit":"b".repeat(40),"prompt_digest":"c".repeat(64),"config_digest":"d".repeat(64),"worktree":root,"run_id":run,"attempt":1,"last_error":null,"launch":null})).unwrap();
        storage::write(&root.join("state/jobs").join(format!("{id}.json")), &job).unwrap();
        let logpath = root
            .join("state/runs")
            .join(&run)
            .join("process/stdout.jsonl");
        fs::create_dir_all(logpath.parent().unwrap()).unwrap();
        let mut log = File::create(logpath).unwrap();
        writeln!(log,"{}",json!({"type":"item.completed","item":{"type":"agent_message","text":"Public progress"}})).unwrap();
        writeln!(
            log,
            "{}",
            json!({"type":"item.completed","item":{"type":"reasoning","text":"PRIVATE_REASONING"}})
        )
        .unwrap();
        writeln!(log,"{}",json!({"type":"item.completed","item":{"type":"command_execution","command":"Authorization: Bearer SECRET"}})).unwrap();
        drop(log);
        let config: Config = serde_json::from_value(json!({"schema_version":1,"workspace":root,"state_dir":root.join("state"),"superpod":root,"codex":"unused","workflow":"unused","daily_seconds":600,"max_agents":1,"task_timeout_seconds":60,"require_latest":false,"models":{},"tools":{}})).unwrap();
        let (updates, _) = tokio::sync::watch::channel(0);
        let (shutdown, _) = tokio::sync::watch::channel(false);
        let web = transport::Web::new(
            hub,
            "127.0.0.1:1234".parse().unwrap(),
            Some(Arc::new(config)),
            Arc::new(RwLock::new(json!({}))),
            updates,
            shutdown,
        );
        let scope = || Scope {
            group_id: "group".into(),
            goal_id: "goal".into(),
            work_id: work.id.clone(),
            attempt: 1,
            before: None,
        };
        let projection = get(&web, scope()).await.unwrap();
        assert_eq!(projection["events"][0]["text"], "Public progress");
        assert_eq!(projection["goal_revision"], goal.revision);
        assert_eq!(projection["run_id"], run);
        assert!(!projection.to_string().contains("PRIVATE"));
        assert!(!projection.to_string().contains("SECRET"));
        let mut wrong = scope();
        wrong.attempt = 2;
        assert!(get(&web, wrong).await.is_err());
        let mut wrong = scope();
        wrong.group_id = "other".into();
        assert!(get(&web, wrong).await.is_err());
    }
    #[test]
    fn native_attempt_and_identity_must_match_the_derived_assignment_job() {
        let mut job: Job = serde_json::from_value(json!({
            "task":{"id":"im-goal-work-0-2","persona_id":"person","role":"research","repository":"superpod","prompt":"private"},
            "persona":{"id":"person","name":"Researcher","revision":1,"soul":"private","memory":null},
            "model":"fixture","source_commit":"a".repeat(40),"superpod_commit":"b".repeat(40),"prompt_digest":"c".repeat(64),"config_digest":"d".repeat(64),
            "worktree":"/private","run_id":"im-goal-work-0-2-attempt-1","attempt":1,"last_error":null,"launch":null
        })).unwrap();
        assert!(validate_job(&job, "im-goal-work-0-2", "person").is_ok());
        assert!(validate_job(&job, "im-goal-work-0-1", "person").is_err());
        assert!(validate_job(&job, "im-goal-work-0-2", "other").is_err());
        job.attempt = 2;
        assert!(validate_job(&job, "im-goal-work-0-2", "person").is_err());
        job.attempt = 1;
        job.run_id = "foreign-attempt-1".into();
        assert!(validate_job(&job, "im-goal-work-0-2", "person").is_err());
    }
    #[test]
    fn cursor_seeks_current_round_without_scanning_historical_goals() {
        for id in ["campaign-000001", "campaign-999999", "A", "goal_z"] {
            let cursor = cursor_before(id);
            assert!(cursor.as_str() < id);
            assert!(cursor.len() <= 100);
        }
        assert!(cursor_before("campaign-999999").as_str() > "campaign-999998");
    }
    #[test]
    fn assignment_scope_rejects_wrong_group_scenario_and_stale_attempt() {
        let mut goal: Goal = serde_json::from_value(json!({
            "input":{"id":"goal","group_id":"group","title":"Research","objective":"Inspect","acceptance":"Evidence","scenario":"research","parameters":{},"assignments":[{"person_id":"person","instruction":"Inspect"}],"max_seconds":180},
            "state":"active","revision":1,"created_ms":0,
            "work":[{"id":"work-0","person_id":"person","instruction":"Inspect","state":"running","attempt":2,"claim_id":null,"deadline_ms":null,"result":null}]
        })).unwrap();
        let mut scope = Scope {
            group_id: "group".into(),
            goal_id: "goal".into(),
            work_id: "work-0".into(),
            attempt: 2,
            before: None,
        };
        assert!(assignment(&scope, &goal).is_ok());
        scope.attempt = 1;
        assert!(assignment(&scope, &goal).is_err());
        scope.attempt = 2;
        scope.group_id = "other".into();
        assert!(assignment(&scope, &goal).is_err());
        scope.group_id = "group".into();
        goal.input.scenario = "collaboration".into();
        assert!(assignment(&scope, &goal).is_err());
    }
}
