//! Adapter contract tests use a declared workflow fixture, never a live model.
use super::research_scenario::*;
use super::*;
use crystal::{AssignmentInput, GoalInput, GoalState, WorkItem, WorkState};
use im_service::ScenarioAdapter;
use std::os::unix::fs::PermissionsExt;
struct Scratch(std::path::PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "im-scenario-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[test]
fn group_objective_becomes_a_read_only_person_bound_research_task() {
    let temp = Scratch::new();
    let c: Config = serde_json::from_value(json!({"schema_version":1,"workspace":temp.path(),"state_dir":temp.path().join("state"),"superpod":temp.path(),"codex":"unused","workflow":"unused","daily_seconds":600,"max_agents":2,"task_timeout_seconds":60,"require_latest":false,"models":{"research":"fixture"},"tools":{}})).unwrap();
    let person = runtime::people::default_id(&c, "research").unwrap();
    let input = GoalInput {
        id: "goal".into(),
        group_id: "team".into(),
        title: "CLI comparison".into(),
        objective: "Compare failure recovery".into(),
        acceptance: "Cite pinned evidence and limitations".into(),
        scenario: "research".into(),
        parameters: json!({"repository":"superpod"}),
        assignments: vec![AssignmentInput {
            person_id: person.clone(),
            instruction: "Inspect restart behavior".into(),
        }],
        max_seconds: 180,
    };
    let adapter = Research(Arc::new(c));
    adapter.validate(&input).unwrap();
    let work = WorkItem {
        id: "work-0".into(),
        person_id: person.clone(),
        instruction: input.assignments[0].instruction.clone(),
        state: WorkState::Ready,
        attempt: 1,
        claim_id: None,
        deadline_ms: None,
        result: None,
    };
    let mut goal = crystal::Goal {
        input,
        state: GoalState::Active,
        revision: 1,
        created_ms: 0,
        work: vec![work.clone()],
    };
    let task = task(&goal, &work, &[]).unwrap();
    assert_eq!(task.persona_id.as_deref(), Some(person.as_str()));
    assert_eq!(task.role, "research");
    assert!(!task.write);
    assert!(!task.use_memory);
    assert!(task.prompt.contains("Compare failure recovery"));
    assert!(task.prompt.contains("Cite pinned evidence"));
    assert!(task.prompt.contains("Inspect restart behavior"));
    let mut retry = work;
    retry.attempt = 2;
    assert_ne!(task.id, job_id(&goal, &retry));
    goal.input.parameters["interaction"] = json!(contracts::ResearchInteraction::Discussion);
    let mut next = goal.work[0].clone();
    next.id = "work-1".into();
    goal.work.push(next.clone());
    assert!(turn_ready(&goal, &goal.work[0]).unwrap());
    assert!(!turn_ready(&goal, &next).unwrap());
    goal.work[0].state = WorkState::Submitted;
    assert!(turn_ready(&goal, &next).unwrap());
    goal.work[0].state = WorkState::Running;
    assert!(!turn_ready(&goal, &next).unwrap());
    goal.input.parameters["interaction"] = json!(contracts::ResearchInteraction::Parallel);
    assert!(turn_ready(&goal, &next).unwrap());
    goal.input.parameters["interaction"] = json!("unknown");
    assert!(turn_ready(&goal, &next).is_err());
    goal.input.parameters = json!({"repository":"superpod"});
    goal.input.assignments[0].person_id = "no-executor".into();
    assert!(adapter.validate(&goal.input).is_err());
    goal.input.parameters = json!({"repository":"/arbitrary/repo"});
    assert!(adapter.validate(&goal.input).is_err());
}
#[test]
fn successful_workflow_status_without_a_matching_committed_receipt_cannot_submit_to_im() {
    let temp = Scratch::new();
    let root = temp.path();
    let binary = root.join("workflow-fixture");
    fs::write(
        &binary,
        format!(
            "#!/bin/sh\ncat '{}'\n",
            root.join("response.json").display()
        ),
    )
    .unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
    let c:Config = serde_json::from_value(json!({"schema_version":1,"workspace":root,"state_dir":root.join("state"),"superpod":root,"codex":"unused","workflow":binary,"daily_seconds":600,"max_agents":1,"task_timeout_seconds":60,"require_latest":false,"models":{},"tools":{}})).unwrap();
    fs::create_dir_all(c.state_dir.join("workflow")).unwrap();
    let job:Job = serde_json::from_value(json!({"task":{"id":"im-job","role":"research","repository":"superpod","prompt":"frozen"},"model":"fixture","source_commit":"a".repeat(40),"superpod_commit":"b".repeat(40),"prompt_digest":"c".repeat(64),"config_digest":"d".repeat(64),"worktree":root,"run_id":"im-job-attempt-1","attempt":1,"last_error":null,"launch":null})).unwrap();
    storage::write(&c.state_dir.join("jobs/im-job.json"), &job).unwrap();
    let mut receipt = json!({"agent_report":{"summary":"Pinned research result","findings":[],"sources":[],"limitations":["synthetic workflow fixture"],"next_tasks":[]},"source_commit":job.source_commit,"superpod_commit":job.superpod_commit,"prompt_digest":job.prompt_digest,"model":job.model,"research_inputs":null,"backend":null});
    let persist = |receipt: &Value| {
        storage::write(&root.join("response.json"),&json!({"ok":true,"result":{"status":"succeeded","frames":{"1":{"nodes":{"task":{"outputs":{"result":receipt.to_string()}}}}}}})).unwrap()
    };
    persist(&receipt);
    let result = observe_result(&c, "im-job").unwrap().unwrap();
    assert!(result.succeeded);
    assert_eq!(result.summary, "Pinned research result");
    assert_eq!(result.evidence["source_commit"], job.source_commit);
    assert!(result.evidence["receipt_sha256"].is_string());
    assert!(
        result.evidence["policy_digest"].is_null(),
        "legacy receipts cannot attest policy"
    );
    receipt["receipt_protocol"] = json!(2);
    receipt["config_digest"] = json!(job.config_digest);
    persist(&receipt);
    assert_eq!(
        observe_result(&c, "im-job").unwrap().unwrap().evidence["policy_digest"],
        job.config_digest
    );
    receipt["config_digest"] = json!("wrong-policy");
    persist(&receipt);
    assert!(observe_result(&c, "im-job").is_err());
    receipt["config_digest"] = json!(job.config_digest);
    receipt["receipt_protocol"] = json!(999);
    persist(&receipt);
    assert!(observe_result(&c, "im-job").is_err());
    receipt["receipt_protocol"] = json!(2);
    receipt["agent_report"]["findings"] = json!(vec!["x".repeat(1200); 12]);
    receipt["agent_report"]["limitations"] = json!(vec!["y".repeat(1200); 8]);
    persist(&receipt);
    let bounded = observe_result(&c, "im-job").unwrap().unwrap();
    assert_eq!(bounded.evidence["report_truncated"], true);
    assert!(serde_json::to_vec(&bounded.evidence).unwrap().len() <= 16000);
    assert_eq!(bounded.evidence["policy_digest"], job.config_digest);
    assert_eq!(
        bounded.evidence["receipt_sha256"],
        storage::digest(&serde_json::to_vec(&receipt).unwrap())
    );
    receipt["source_commit"] = json!("wrong-source");
    persist(&receipt);
    assert!(observe_result(&c, "im-job").is_err());
    storage::write(
        &root.join("response.json"),
        &json!({"ok":true,"result":{"status":"running"}}),
    )
    .unwrap();
    assert!(observe_result(&c, "im-job").unwrap().is_none());
}

#[tokio::test]
async fn admission_failure_is_unavailable_with_private_diagnostics_and_no_acceptance() {
    let temp = Scratch::new();
    let c: Config = serde_json::from_value(json!({"schema_version":1,"workspace":temp.path(),"state_dir":temp.path().join("state"),"superpod":temp.path(),"codex":"unused","workflow":"unused","daily_seconds":600,"max_agents":1,"task_timeout_seconds":60,"require_latest":true,"models":{"research":"fixture"},"tools":{}})).unwrap();
    let person = runtime::people::default_id(&c, "research").unwrap();
    let hub =
        im_storage::Client::from(crystal::Hub::open(&temp.path().join("collaboration")).unwrap());
    hub.register_operator().await.unwrap();
    hub.save_person(crystal::Person {
        id: person.clone(),
        name: "Researcher".into(),
        application_id: Some("research".into()),
    })
    .await
    .unwrap();
    hub.create_group(crystal::NewGroup {
        id: "research-test".into(),
        title: "Research".into(),
        topic: "Admission feedback".into(),
        kind: crystal::GroupKind::Conversation,
        private: false,
        members: vec!["operator".into(), person.clone()],
    })
    .await
    .unwrap();
    let input = GoalInput {
        id: "admission-test".into(),
        group_id: "research-test".into(),
        title: "Verify admission".into(),
        objective: "Check unavailable input evidence".into(),
        acceptance: "A failed admission must not claim readiness".into(),
        scenario: "research".into(),
        parameters: json!({"repository":"superpod"}),
        assignments: vec![AssignmentInput {
            person_id: person,
            instruction: "Read-only research".into(),
        }],
        max_seconds: 180,
    };
    let initial = hub
        .change_goal(crystal::GoalChange::Create { goal: input })
        .await
        .unwrap();
    let job = job_id(&initial, &initial.work[0]);
    let config = Arc::new(c);
    let (stop, receiver) = tokio::sync::watch::channel(false);
    let worker = tokio::spawn(run(hub.clone(), config.clone(), 15, receiver));
    let goal = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let page = hub
                .goals("research-test", "research", "", true)
                .await
                .unwrap();
            let goal: crystal::Goal = serde_json::from_value(page["goals"][0].clone()).unwrap();
            if goal.work[0].state == WorkState::Failed {
                break goal;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap();
    stop.send(true).unwrap();
    worker.await.unwrap();
    assert_eq!(goal.state, GoalState::Active);
    let result = goal.work[0].result.as_ref().unwrap();
    assert_eq!(
        result.code,
        Some(contracts::WorkResultCode::AdmissionFailed)
    );
    assert!(!result.succeeded);
    let status = Research(config.clone()).status();
    assert_eq!(status["available"], false);
    assert_eq!(
        status["status"],
        contracts::ScenarioStatus::Unavailable.as_str()
    );
    let diagnostic: Value = storage::read(
        &config
            .state_dir
            .join("private/im-admission")
            .join(format!("{job}.json")),
    )
    .unwrap();
    assert!(
        diagnostic["error"]
            .as_str()
            .unwrap()
            .contains("missing skills manifest")
    );
    assert!(
        !serde_json::to_string(&result)
            .unwrap()
            .contains("missing skills manifest")
    );
    assert!(!status.to_string().contains("missing skills manifest"));
    assert!(
        !config
            .state_dir
            .join("jobs")
            .join(format!("{job}.json"))
            .exists()
    );
}
