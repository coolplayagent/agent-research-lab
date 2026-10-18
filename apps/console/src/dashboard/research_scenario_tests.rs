//! Adapter contract tests use a declared workflow fixture, never a live model.
use super::research_scenario::*;
use super::*;
use collaboration_api::ScenarioAdapter;
use collaboration_model::{AssignmentInput, GoalInput, GoalState, WorkItem, WorkState};
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
fn software_stages_require_successful_dependencies_regardless_of_assignment_order() {
    let mut goal: collaboration_model::Goal = serde_json::from_value(json!({
        "input":{"id":"software-goal","group_id":"team","title":"Improve behavior",
            "objective":"Fix the proven regression","acceptance":"Independent verification passes",
            "scenario":"software-evolution","max_seconds":240,
            "parameters":{"repository":"superpod","roles":{"reviewer":"review","builder":"implement","researcher":"research"},"delivery_policy_digest":"a".repeat(64)},
            "assignments":[{"person_id":"reviewer","instruction":"Review"},{"person_id":"builder","instruction":"Implement"},{"person_id":"researcher","instruction":"Research"}]},
        "state":"active","revision":1,"created_ms":0,
        "work":[
            {"id":"work-0","person_id":"reviewer","instruction":"Review","state":"ready","attempt":1,"claim_id":null,"deadline_ms":null,"result":null},
            {"id":"work-1","person_id":"builder","instruction":"Implement","state":"ready","attempt":1,"claim_id":null,"deadline_ms":null,"result":null},
            {"id":"work-2","person_id":"researcher","instruction":"Research","state":"ready","attempt":1,"claim_id":null,"deadline_ms":null,"result":null}
        ]})).unwrap();
    let ready = |goal: &collaboration_model::Goal, index| {
        super::software_scenario::ready(goal, &goal.work[index]).unwrap()
    };
    assert!(ready(&goal, 2));
    assert!(!ready(&goal, 1));
    assert!(!ready(&goal, 0));
    goal.work[2].state = WorkState::Submitted;
    assert!(
        !ready(&goal, 1),
        "a state without a successful receipt is insufficient"
    );
    goal.work[2].result = Some(collaboration_model::WorkResult {
        code: None,
        succeeded: true,
        summary: "Reproducible regression found".into(),
        evidence: json!({}),
    });
    assert!(ready(&goal, 1));
    assert!(!ready(&goal, 0));
    let implementation = super::software_scenario::task(&goal, &goal.work[1], &[]).unwrap();
    assert!(implementation.write);
    assert_eq!(
        implementation.dependencies,
        vec![job_id(&goal, &goal.work[2])]
    );
    assert!(
        implementation
            .prompt
            .contains("Reproducible regression found")
    );
    goal.work[1].state = WorkState::Submitted;
    goal.work[1].result = Some(collaboration_model::WorkResult {
        code: None,
        succeeded: true,
        summary: "Candidate includes focused verification".into(),
        evidence: json!({}),
    });
    assert!(ready(&goal, 0));
    let review = super::software_scenario::task(&goal, &goal.work[0], &[]).unwrap();
    assert!(!review.write);
    assert_eq!(
        review.dependencies,
        vec![job_id(&goal, &goal.work[1]), job_id(&goal, &goal.work[2])]
    );
    assert!(
        review
            .prompt
            .contains("Candidate includes focused verification")
    );
    goal.work[2].state = WorkState::Failed;
    assert!(!ready(&goal, 0));
    assert!(!ready(&goal, 1));
}

#[test]
fn native_scope_retains_completed_dependencies_and_rejects_other_goal_attempts() {
    let temp = Scratch::new();
    let root = temp.path();
    let c: Config = serde_json::from_value(json!({"schema_version":1,"workspace":root,"state_dir":root.join("state"),"superpod":root,"codex":"unused","workflow":"unused","daily_seconds":600,"max_agents":1,"task_timeout_seconds":60,"require_latest":false,"models":{},"tools":{}})).unwrap();
    let mut owners = BTreeMap::new();
    for (id, dependencies) in [
        ("researcher", vec![]),
        ("implementer", vec!["researcher"]),
        ("reviewer", vec!["implementer", "researcher"]),
    ] {
        let job: Job = serde_json::from_value(json!({"task":{"id":id,"role":"research","repository":"superpod","prompt":"frozen","dependencies":dependencies},"model":"fixture","source_commit":"a".repeat(40),"superpod_commit":"b".repeat(40),"prompt_digest":"c".repeat(64),"config_digest":"d".repeat(64),"worktree":root,"run_id":format!("{id}-attempt-1"),"attempt":1,"last_error":null,"launch":null})).unwrap();
        storage::write(&c.state_dir.join("jobs").join(format!("{id}.json")), &job).unwrap();
        owners.insert(id.into(), "current-goal".into());
    }
    let pending = vec![("reviewer".into(), 1000)];
    assert_eq!(
        execution_scope(&c, &pending, &owners).unwrap(),
        vec!["implementer", "researcher", "reviewer"]
    );
    owners.insert("researcher".into(), "another-goal".into());
    assert!(execution_scope(&c, &pending, &owners).is_err());
    owners.remove("researcher");
    assert!(execution_scope(&c, &pending, &owners).is_err());
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
    let mut goal = collaboration_model::Goal {
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
    goal.work[0].result = Some(collaboration_model::WorkResult {
        code: None,
        succeeded: true,
        summary: "Prior recorded contribution outside the recent message window".into(),
        evidence: json!({}),
    });
    assert!(
        super::research_scenario::task(&goal, &next, &[])
            .unwrap()
            .prompt
            .contains("Prior recorded contribution outside the recent message window")
    );
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
    assert!(
        observe_result(&c, "im-job").is_err(),
        "retired receipts cannot attest policy"
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
async fn running_receipts_reconcile_after_sibling_failure_without_admitting_dependents() {
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
    let c: Config = serde_json::from_value(json!({"schema_version":1,"workspace":root,
        "state_dir":root.join("state"),"superpod":root,"codex":"unused","workflow":binary,
        "daily_seconds":600,"max_agents":4,"task_timeout_seconds":60,"require_latest":false,
        "models":{},"tools":{}}))
    .unwrap();
    fs::create_dir_all(c.state_dir.join("workflow")).unwrap();
    let c = Arc::new(c);
    let hub = collaboration_storage_service::client(
        collaboration_store::Hub::open(&root.join("collaboration")).unwrap(),
    );
    hub.register_operator().await.unwrap();
    let people = ["researcher", "sibling", "builder", "reviewer"];
    for person in people {
        hub.save_person(collaboration_model::Person {
            id: person.into(),
            name: person.into(),
            application_id: Some("research".into()),
        })
        .await
        .unwrap();
    }
    hub.create_group(collaboration_model::NewGroup {
        id: "team".into(),
        title: "Software research".into(),
        topic: "Receipt reconciliation".into(),
        kind: collaboration_model::GroupKind::Conversation,
        private: false,
        members: std::iter::once("operator")
            .chain(people)
            .map(str::to_owned)
            .collect(),
    })
    .await
    .unwrap();
    let input = GoalInput {
        id: "software-goal".into(),
        group_id: "team".into(),
        title: "Reconcile research".into(),
        objective: "Observe every started task before settling the round".into(),
        acceptance: "Only current committed receipts can submit".into(),
        scenario: "software-evolution".into(),
        max_seconds: 180,
        parameters: json!({"repository":"superpod","delivery_policy_digest":"a".repeat(64),
            "roles":{"researcher":"research","sibling":"research","builder":"implement","reviewer":"review"}}),
        assignments: people
            .iter()
            .map(|person| AssignmentInput {
                person_id: (*person).into(),
                instruction: "Inspect the frozen evidence".into(),
            })
            .collect(),
    };
    let mut goal = hub
        .change_goal(collaboration_model::GoalChange::Create { goal: input })
        .await
        .unwrap();
    for index in [0, 1] {
        let work = &goal.work[index];
        goal = hub
            .work_as(
                &work.person_id,
                collaboration_model::WorkChange::Claim {
                    attempt: work.attempt,
                    goal_id: goal.input.id.clone(),
                    work_id: work.id.clone(),
                    claim_id: "research-attempt-1".into(),
                },
            )
            .await
            .unwrap();
    }
    let failure = collaboration_model::WorkResult {
        code: Some(contracts::WorkResultCode::Failed),
        succeeded: false,
        summary: "Declared sibling failure fixture".into(),
        evidence: json!({"fixture":true}),
    };
    goal = hub
        .work_as(
            "sibling",
            collaboration_model::WorkChange::Submit {
                attempt: 1,
                goal_id: goal.input.id.clone(),
                work_id: "work-1".into(),
                claim_id: "research-attempt-1".into(),
                result: failure.clone(),
            },
        )
        .await
        .unwrap();
    let stale_running = goal.clone();
    let id = job_id(&goal, &goal.work[0]);
    let job: Job = serde_json::from_value(json!({"task":{"id":id,"role":"research",
        "repository":"superpod","prompt":"frozen"},"model":"fixture",
        "source_commit":"a".repeat(40),"superpod_commit":"b".repeat(40),
        "prompt_digest":"c".repeat(64),"config_digest":"d".repeat(64),"worktree":root,
        "run_id":format!("{id}-attempt-1"),"attempt":1,"last_error":null,"launch":null}))
    .unwrap();
    storage::write(&c.state_dir.join("jobs").join(format!("{id}.json")), &job).unwrap();
    let response = root.join("response.json");
    storage::write(
        &response,
        &json!({"ok":true,"result":{"status":"succeeded"}}),
    )
    .unwrap();
    assert!(
        advance(&hub, c.clone(), &goal, &goal.work[0])
            .await
            .unwrap()
            .is_some(),
        "successful status without a committed receipt remains unresolved"
    );
    let read_goal = async || -> collaboration_model::Goal {
        let page = hub
            .goals("team", "software-evolution", "", true)
            .await
            .unwrap();
        serde_json::from_value(page["goals"][0].clone()).unwrap()
    };
    assert_eq!(read_goal().await.work[0].state, WorkState::Running);

    let receipt = json!({"agent_report":{"summary":"Pinned research result","findings":[],
        "sources":[],"limitations":["synthetic workflow fixture"],"next_tasks":[]},
        "source_commit":job.source_commit,"superpod_commit":job.superpod_commit,
        "prompt_digest":job.prompt_digest,"model":job.model,"research_inputs":null,"backend":null,
        "receipt_protocol":2,"config_digest":job.config_digest});
    storage::write(
        &response,
        &json!({"ok":true,"result":{"status":"succeeded", "frames":{
        "1":{"nodes":{"task":{"outputs":{"result":receipt.to_string()}}}}}}}),
    )
    .unwrap();
    storage::write(
        &c.state_dir.join("private/activation-drain"),
        &json!({"fixture":true}),
    )
    .unwrap();
    assert!(
        advance(&hub, c.clone(), &goal, &goal.work[0])
            .await
            .unwrap()
            .is_none()
    );
    goal = read_goal().await;
    assert_eq!(goal.state, GoalState::Active);
    assert_eq!(goal.work[0].state, WorkState::Submitted);
    let result = goal.work[0].result.as_ref().unwrap();
    assert!(result.succeeded);
    assert_eq!(
        result.evidence["receipt_sha256"],
        storage::digest(&serde_json::to_vec(&receipt).unwrap())
    );
    assert_eq!(result.evidence["policy_digest"], job.config_digest);
    assert_eq!(goal.work[1].state, WorkState::Failed);
    assert_eq!(goal.work[1].result.as_ref(), Some(&failure));
    fs::remove_file(c.state_dir.join("private/activation-drain")).unwrap();
    for work in &goal.work[2..] {
        assert!(
            advance(&hub, c.clone(), &goal, work)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            !c.state_dir
                .join("jobs")
                .join(format!("{}.json", job_id(&goal, work)))
                .exists()
        );
    }
    let current = read_goal().await;
    assert_eq!(current.revision, goal.revision);
    assert!(
        current.work[2..]
            .iter()
            .all(|work| work.state == WorkState::Ready
                && work.claim_id.is_none()
                && work.result.is_none())
    );

    hub.change_goal(collaboration_model::GoalChange::Retry {
        goal_id: goal.input.id.clone(),
        revision: goal.revision,
        work_id: "work-0".into(),
    })
    .await
    .unwrap();
    let error = advance(&hub, c, &stale_running, &stale_running.work[0])
        .await
        .unwrap_err();
    assert!(error.to_string().contains("attempt has been superseded"));
    let current = read_goal().await;
    assert_eq!(current.work[0].attempt, 2);
    assert_eq!(current.work[0].state, WorkState::Ready);
    assert!(current.work[0].result.is_none());
    assert_eq!(current.work[1].result.as_ref(), Some(&failure));
}

#[tokio::test]
async fn admission_failure_is_unavailable_with_private_diagnostics_and_no_acceptance() {
    let temp = Scratch::new();
    let c: Config = serde_json::from_value(json!({"schema_version":1,"workspace":temp.path(),"state_dir":temp.path().join("state"),"superpod":temp.path(),"codex":"unused","workflow":"unused","daily_seconds":600,"max_agents":1,"task_timeout_seconds":60,"require_latest":true,"models":{"research":"fixture"},"tools":{}})).unwrap();
    let person = runtime::people::default_id(&c, "research").unwrap();
    let hub = collaboration_storage_service::client(
        collaboration_store::Hub::open(&temp.path().join("collaboration")).unwrap(),
    );
    hub.register_operator().await.unwrap();
    hub.save_person(collaboration_model::Person {
        id: person.clone(),
        name: "Researcher".into(),
        application_id: Some("research".into()),
    })
    .await
    .unwrap();
    hub.create_group(collaboration_model::NewGroup {
        id: "research-test".into(),
        title: "Research".into(),
        topic: "Admission feedback".into(),
        kind: collaboration_model::GroupKind::Conversation,
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
        .change_goal(collaboration_model::GoalChange::Create { goal: input })
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
            let goal: collaboration_model::Goal =
                serde_json::from_value(page["goals"][0].clone()).unwrap();
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

#[test]
fn exhausted_native_rejection_fails_the_goal_without_exposing_private_diagnostics() {
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
    let c: Config = serde_json::from_value(json!({"schema_version":1,"workspace":root,
        "state_dir":root.join("state"),"superpod":root,"codex":"unused","workflow":binary,
        "daily_seconds":600,"max_agents":1,"task_timeout_seconds":60,"require_latest":false,
        "models":{},"tools":{}}))
    .unwrap();
    fs::create_dir_all(c.state_dir.join("workflow")).unwrap();
    let mut job: Job = serde_json::from_value(json!({"task":{"id":"blocked-job","role":"research",
        "repository":"superpod","prompt":"frozen","max_attempts":1},"model":"fixture",
        "source_commit":"a".repeat(40),"superpod_commit":"b".repeat(40),"prompt_digest":"c".repeat(64),
        "config_digest":"d".repeat(64),"worktree":root,"run_id":"blocked-job-attempt-1","attempt":1,
        "last_error":"launch failed: PRIVATE_HOST_DIAGNOSTIC","launch":null,"retry_after":null})).unwrap();
    let persist =
        |job: &Job| storage::write(&c.state_dir.join("jobs/blocked-job.json"), job).unwrap();
    let response = |state| {
        storage::write(&root.join("response.json"), &json!({"ok":true,
        "result":{"status":state,"frames":{"1":{"nodes":{"task":{"state":{"state":"task_ready"}}}}}}})).unwrap()
    };
    response(contracts::WorkflowState::Running.as_str());
    persist(&job);
    let result = observe_result(&c, "blocked-job").unwrap().unwrap();
    assert!(!result.succeeded);
    assert_eq!(result.code, Some(contracts::WorkResultCode::Failed));
    assert_eq!(result.evidence["job_id"], job.task.id);
    assert_eq!(result.evidence["attempt"], 1);
    assert_eq!(result.evidence["policy_digest"], job.config_digest);
    assert!(
        !serde_json::to_string(&result)
            .unwrap()
            .contains("PRIVATE_HOST_DIAGNOSTIC")
    );
    let retained: Job = storage::read(&c.state_dir.join("jobs/blocked-job.json")).unwrap();
    assert_eq!(retained.last_error, job.last_error);

    job.task.max_attempts = Some(2);
    persist(&job);
    assert!(
        observe_result(&c, "blocked-job").unwrap().is_none(),
        "an explicit retry remains available"
    );
    job.task.max_attempts = Some(1);
    job.retry_after = Some(i64::MAX);
    persist(&job);
    assert!(
        observe_result(&c, "blocked-job").unwrap().is_none(),
        "retry waiting is not terminal"
    );
    job.retry_after = None;
    job.launch = Some(
        serde_json::from_value(json!({"pid":0,"process_start":"unavailable","lease":{},
        "attempt":{},"log_dir":root}))
        .unwrap(),
    );
    persist(&job);
    assert!(
        observe_result(&c, "blocked-job").unwrap().is_none(),
        "retained launch needs reconciliation"
    );
    job.launch = None;
    persist(&job);
    response("unsupported-future-state");
    assert!(
        observe_result(&c, "blocked-job").unwrap().is_none(),
        "unknown authority is not a terminal receipt"
    );
    response(contracts::WorkflowState::Running.as_str());
    job.last_error = None;
    persist(&job);
    assert!(
        observe_result(&c, "blocked-job").unwrap().is_none(),
        "ready work must remain eligible"
    );
    storage::write(&c.state_dir.join("paused"), &json!({})).unwrap();
    assert!(
        observe_result(&c, "blocked-job").unwrap().is_none(),
        "paused admission must remain resumable"
    );
}
