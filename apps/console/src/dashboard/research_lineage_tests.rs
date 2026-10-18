use super::*;
fn configuration(root: &Path) -> Config {
    serde_json::from_value(json!({"schema_version":1,"workspace":root,"state_dir":root.join("state"),"superpod":root,"codex":"unused","workflow":"unused","daily_seconds":600,"max_agents":2,"task_timeout_seconds":60,"require_latest":false,"models":{"research":"fixture"},"tools":{}})).unwrap()
}
async fn goal(hub: &collaboration_client::Client, id: &str) -> Goal {
    hub.change_goal(collaboration_model::GoalChange::Create {
        goal: collaboration_model::GoalInput {
            id: id.into(),
            group_id: "group".into(),
            title: id.into(),
            objective: "Inspect recorded source".into(),
            acceptance: "Reproducible findings".into(),
            scenario: "research".into(),
            parameters: json!({"repository":"fixture"}),
            max_seconds: 180,
            assignments: ["one", "two"]
                .into_iter()
                .map(|person| collaboration_model::AssignmentInput {
                    person_id: person.into(),
                    instruction: "Review bounded evidence".into(),
                })
                .collect(),
        },
    })
    .await
    .unwrap()
}
fn write_jobs(c: &Config, goal: &Goal) {
    for (index, work) in goal.work.iter().enumerate() {
        let id = super::super::research_scenario::job_id(goal, work);
        let task = super::super::research_scenario::task(goal, work, &[]).unwrap();
        let job: Job = serde_json::from_value(json!({"task":task,"model":"fixture","source_commit":"a".repeat(40),"superpod_commit":"b".repeat(40),"prompt_digest":if index == 0 {"c".repeat(64)} else {"e".repeat(64)},"config_digest":"d".repeat(64),"worktree":c.workspace,"run_id":format!("{id}-attempt-1"),"attempt":1,"last_error":null,"launch":null})).unwrap();
        storage::write(&c.state_dir.join("jobs").join(format!("{id}.json")), &job).unwrap();
    }
}
#[tokio::test]
async fn repeated_versions_keep_separate_attempt_evidence_without_duplicate_nodes() {
    let root = std::env::temp_dir().join(format!(
        "lineage-sync-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let c = configuration(&root);
    let hub = collaboration_storage_service::client(
        collaboration_store::Hub::open(&root.join("collaboration")).unwrap(),
    );
    hub.register_operator().await.unwrap();
    hub.register(vec!["one".into(), "two".into()])
        .await
        .unwrap();
    hub.create_group(collaboration_model::NewGroup {
        id: "group".into(),
        title: "Research".into(),
        topic: "Fixture".into(),
        private: false,
        kind: collaboration_model::GroupKind::Conversation,
        members: vec!["operator".into(), "one".into(), "two".into()],
    })
    .await
    .unwrap();
    let first = goal(&hub, "first").await;
    let second = goal(&hub, "second").await;
    write_jobs(&c, &first);
    write_jobs(&c, &second);
    assert_eq!(sync(&hub, &c).await.unwrap(), 2);
    assert_eq!(sync(&hub, &c).await.unwrap(), 2);
    let topic_id = identity("research-topic", &["group"]);
    let graph = hub.graph(&topic_id, "", 0).await.unwrap();
    assert_eq!(graph["nodes"].as_array().unwrap().len(), 1);
    assert_eq!(graph["nodes"][0]["assessment_count"], 2);
    assert_eq!(
        graph["nodes"][0]["status"],
        contracts::EvolutionState::Pending.as_str()
    );
    let assessments = graph["nodes"][0]["assessments"].as_array().unwrap();
    assert_ne!(
        assessments[0]["assessment"]["input"]["pins_by_work"]["work-0"]["prompt"],
        assessments[0]["assessment"]["input"]["pins_by_work"]["work-1"]["prompt"]
    );
    let path = c.state_dir.join("jobs").join(format!(
        "{}.json",
        super::super::research_scenario::job_id(&first, &first.work[0])
    ));
    let mut job: Job = storage::read(&path).unwrap();
    job.source_commit = "f".repeat(40);
    storage::write(&path, &job).unwrap();
    assert!(
        pins_for_goal(&c, &first).is_none(),
        "mixed source versions must not create lineage"
    );
    job.source_commit = "a".repeat(40);
    job.task.persona_id = Some("two".into());
    storage::write(&path, &job).unwrap();
    assert!(
        pins_for_goal(&c, &first).is_none(),
        "a mismatched worker identity cannot supply pins"
    );
    fs::remove_dir_all(root).unwrap();
}
