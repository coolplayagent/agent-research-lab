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
    let hub = collaboration_storage_service::client(
        collaboration_store::Hub::open(&root.join("collaboration")).unwrap(),
    );
    hub.register_operator().await.unwrap();
    hub.save_person(collaboration_model::Person {
        id: "person".into(),
        name: "Researcher".into(),
        application_id: Some("research".into()),
    })
    .await
    .unwrap();
    hub.create_group(collaboration_model::NewGroup {
        id: "group".into(),
        title: "Research".into(),
        topic: "Process".into(),
        kind: collaboration_model::GroupKind::Conversation,
        private: false,
        members: vec!["operator".into(), "person".into()],
    })
    .await
    .unwrap();
    let input: collaboration_model::GoalInput = serde_json::from_value(json!({"id":"goal","group_id":"group","title":"Research","objective":"Inspect","acceptance":"Evidence","scenario":"research","parameters":{},"assignments":[{"person_id":"person","instruction":"Inspect"}],"max_seconds":180})).unwrap();
    let goal = hub
        .change_goal(collaboration_model::GoalChange::Create { goal: input })
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
    writeln!(
        log,
        "{}",
        json!({"type":"item.completed","item":{"type":"agent_message","text":"Public progress"}})
    )
    .unwrap();
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
