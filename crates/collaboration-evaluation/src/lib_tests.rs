use super::*;
fn fixture() -> (tempfile::TempDir, Config, Plan, Citation) {
    fixture_protocol(PROTOCOL)
}
fn fixture_protocol(protocol: &str) -> (tempfile::TempDir, Config, Plan, Citation) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let state = root.join("state");
    let repo = root.join("superpod");
    fs::create_dir_all(repo.join("knowledge")).unwrap();
    fs::create_dir_all(&state).unwrap();
    let content = "# Shared knowledge\nRecovery needs observable effects.\n";
    fs::write(repo.join("knowledge/index.md"), content).unwrap();
    for args in [
        vec!["init", "-q"],
        vec!["add", "knowledge/index.md"],
        vec![
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            "fixture",
        ],
    ] {
        assert!(
            std::process::Command::new("git")
                .args(args)
                .current_dir(&repo)
                .status()
                .unwrap()
                .success()
        );
    }
    let commit = String::from_utf8(
        std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&repo)
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim()
    .to_owned();
    let c:Config=serde_json::from_value(json!({"schema_version":1,"workspace":root,"state_dir":state,"superpod":repo,"codex":"fixture","workflow":"/fixture/workflow","daily_seconds":7200,"max_agents":3,"task_timeout_seconds":300,"models":{"research":"model-a","review":"model-b","implement":"model-c"},"tools":{},"require_latest":false,"skills_manifest":null})).unwrap();
    let r = PlanRequest {
        schema_version: 1,
        experiment_id: "pilot-one".into(),
        task_family: "recovery".into(),
        trial_id: "instance-one".into(),
        question: "How should interrupted effects be reconciled?".into(),
        primary_hypothesis: "Peer counterexamples improve justified revisions".into(),
        policy_version: "human-rubric-v1".into(),
        max_claims: 3,
        task_timeout_seconds: 300,
        first_arm: Arm::Baseline,
        knowledge_sources: vec!["knowledge/index.md".into()],
        slots: (0..8)
            .map(|i| Slot {
                id: format!("slot-{i}"),
                role: if i % 2 == 0 { "research" } else { "review" }.into(),
                model: if i % 2 == 0 { "model-a" } else { "model-b" }.into(),
                perspective: format!("Perspective {i}"),
                parent_prompt_version: None,
            })
            .collect(),
    };
    let prompts: Vec<_> = r
        .slots
        .iter()
        .map(|slot| {
            PromptVersion::new(PromptDefinition {
                parent_versions: vec![],
                role: slot.role.clone(),
                task_kind: protocol.into(),
                content: specialist_prompt(slot),
                change_reason: "fixture".into(),
                failure_conditions: vec!["unsupported evidence".into()],
            })
            .unwrap()
        })
        .collect();
    let mut registry = PromptRegistry::default();
    for p in &prompts {
        registry.register(p.clone()).unwrap();
    }
    storage::write(&state.join("evolution/prompts.json"), &registry).unwrap();
    let inputs:ExecutionInputs=serde_json::from_value(json!({"repositories":{"superpod":{"upstream":{"repository":"stevetdp/superpod","default_branch":"main","commit":commit,"checked_at":"2026-10-10T00:00:00Z"},"checkout":repo}},"skills":{"schema_version":1,"skills":[]},"insights":"frozen insights","insights_sha256":storage::digest(b"frozen insights")})).unwrap();
    let tasks = build_tasks(&r, &prompts, protocol).unwrap();
    let mut plan = Plan {
        schema_version: 1,
        protocol: protocol.into(),
        request: r,
        config_digest: digest(&c).unwrap(),
        input_digest: input_digest(&inputs).unwrap(),
        inputs,
        prompts,
        tasks,
        digest: String::new(),
    };
    plan.digest = plan_digest(&plan).unwrap();
    let citation = Citation {
        repository: "superpod".into(),
        commit,
        path: "knowledge/index.md".into(),
        start_line: 2,
        end_line: 2,
        sha256: storage::digest(content.as_bytes()),
        quote: "Recovery needs observable effects.".into(),
    };
    (temp, c, plan, citation)
}
fn card(citation: &Citation) -> ClaimCard {
    ClaimCard {
        id: "claim-one".into(),
        text: "Recovery requires observability.".into(),
        evidence: vec![citation.clone()],
        counterexample_to: vec![],
        revises: vec![],
        change: Change::New,
        rationale: "The pinned source explains the condition.".into(),
        knowledge_update: Some(KnowledgeUpdate {
            target_path: "knowledge/index.md".into(),
            proposal: "Add a bounded interrupted-effect experiment.".into(),
        }),
    }
}
fn job_and_receipt(
    c: &Config,
    plan: &Plan,
    entry: &PlannedTask,
    card: &ClaimCard,
) -> (Job, Value, Value) {
    let commit = &plan.inputs.repositories["superpod"].upstream.commit;
    let job = Job {
        persona: None,
        backend: None,
        task: entry.task.clone(),
        model: entry.model.clone(),
        source_commit: commit.clone(),
        baseline_commit: None,
        research_inputs: Some(plan.inputs.clone()),
        superpod_commit: commit.clone(),
        prompt_digest: "frozen-rendered-prompt".into(),
        config_digest: plan.config_digest.clone(),
        worktree: c.state_dir.join("worktrees").join(&entry.task.id),
        run_id: format!("{}-attempt-1", entry.task.id),
        attempt: 1,
        last_error: None,
        launch: None,
        retry_after: None,
    };
    let receipt = json!({"agent_report":{"summary":"A bounded fixture finding","findings":[serde_json::to_string(card).unwrap()],"sources":["superpod:knowledge/index.md"],"limitations":["protocol fixture"],"next_tasks":[]},"source_commit":commit,"superpod_commit":commit,"prompt_digest":job.prompt_digest,"model":job.model});
    let state = json!({"status":"succeeded","frames":{"1":{"nodes":{"task":{"outputs":{"result":serde_json::to_string(&receipt).unwrap()}}}}}});
    (job, receipt, state)
}
fn batch_fixture(real_workflow: Option<PathBuf>) -> (tempfile::TempDir, Config, Plan) {
    use std::os::unix::fs::PermissionsExt;
    let (temp, mut c, mut plan, _) = fixture();
    c.workflow = if let Some(binary) = real_workflow {
        binary
    } else {
        let binary = temp.path().join("explicit-fake-workflow");
        fs::write(
            &binary,
            "#!/bin/sh\nprintf '%s\n' '{\"ok\":true,\"result\":{}}'\n",
        )
        .unwrap();
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
        binary
    };
    plan.config_digest = digest(&c).unwrap();
    plan.digest = plan_digest(&plan).unwrap();
    (temp, c, plan)
}
fn verify_partial_batch_replay(c: &Config, plan: &Plan) {
    let first = &plan.tasks[0];
    runtime::enqueue_with_inputs(c, first.task.clone(), Some(plan.inputs.clone())).unwrap();
    let first_path = c
        .state_dir
        .join("jobs")
        .join(format!("{}.json", first.task.id));
    let preserved = fs::read(&first_path).unwrap();
    let mut refreshed = plan.inputs.clone();
    refreshed
        .repositories
        .get_mut("superpod")
        .unwrap()
        .upstream
        .checked_at = "2026-10-11T00:00:00Z".into();
    let mut collections = 0;
    let result = enqueue_plan(c, plan, || {
        collections += 1;
        Ok(refreshed.clone())
    })
    .unwrap();
    assert_eq!(collections, 1);
    assert_eq!(result["registered_tasks"], 32);
    assert_eq!(result["worker_launch_requested"], false);
    assert_eq!(
        fs::read(&first_path).unwrap(),
        preserved,
        "existing original timestamps and prompt digest must remain frozen"
    );
    assert_eq!(fs::read_dir(c.state_dir.join("jobs")).unwrap().count(), 32);
    let second: Job = storage::read(
        &c.state_dir
            .join("jobs")
            .join(format!("{}.json", plan.tasks[1].task.id)),
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(second.research_inputs).unwrap(),
        serde_json::to_value(Some(refreshed.clone())).unwrap()
    );
    let before: Vec<_> = plan
        .tasks
        .iter()
        .map(|entry| {
            fs::read(
                c.state_dir
                    .join("jobs")
                    .join(format!("{}.json", entry.task.id)),
            )
            .unwrap()
        })
        .collect();
    refreshed
        .repositories
        .get_mut("superpod")
        .unwrap()
        .upstream
        .checked_at = "2026-10-12T00:00:00Z".into();
    assert_eq!(enqueue_plan(c, plan, || Ok(refreshed)).unwrap(), result);
    for (entry, bytes) in plan.tasks.iter().zip(before) {
        assert_eq!(
            fs::read(
                c.state_dir
                    .join("jobs")
                    .join(format!("{}.json", entry.task.id))
            )
            .unwrap(),
            bytes
        );
    }
    assert!(
        !c.state_dir.join("runs").exists(),
        "registration never starts a model process"
    );
}
#[test]
fn batch_enqueue_refreshes_once_and_preserves_partial_replay_bindings() {
    let (_temp, c, plan) = batch_fixture(None);
    verify_partial_batch_replay(&c, &plan);
}
#[test]
fn batch_enqueue_rejects_drift_before_registering_any_missing_task() {
    let (_temp, c, plan) = batch_fixture(None);
    let first = &plan.tasks[0];
    runtime::enqueue_with_inputs(&c, first.task.clone(), Some(plan.inputs.clone())).unwrap();
    let mut stale = plan.inputs.clone();
    stale
        .repositories
        .get_mut("superpod")
        .unwrap()
        .upstream
        .commit = "b".repeat(40);
    assert!(enqueue_plan(&c, &plan, || Ok(stale)).is_err());
    assert_eq!(fs::read_dir(c.state_dir.join("jobs")).unwrap().count(), 1);
    let last = plan.tasks.last().unwrap();
    let mut bad: Job = storage::read(
        &c.state_dir
            .join("jobs")
            .join(format!("{}.json", first.task.id)),
    )
    .unwrap();
    bad.task = last.task.clone();
    bad.model = "different".into();
    storage::write(
        &c.state_dir
            .join("jobs")
            .join(format!("{}.json", last.task.id)),
        &bad,
    )
    .unwrap();
    assert!(
        enqueue_plan(&c, &plan, || panic!(
            "mismatched existing job must fail preflight"
        ))
        .is_err()
    );
    assert_eq!(
        fs::read_dir(c.state_dir.join("jobs")).unwrap().count(),
        2,
        "preflight must not fill earlier missing tasks before finding a late mismatch"
    );
    assert!(
        !c.state_dir
            .join("worktrees")
            .join(&plan.tasks[1].task.id)
            .exists()
    );
}
#[test]
fn batch_enqueue_rejects_changed_plan_and_missing_registered_prompt() {
    let (_temp, c, mut plan) = batch_fixture(None);
    plan.tasks[0].task.prompt.push_str("changed");
    assert!(enqueue_plan(&c, &plan, || panic!("invalid plan must not refresh")).is_err());
    let (_temp, c, plan) = batch_fixture(None);
    fs::remove_file(c.state_dir.join("evolution/prompts.json")).unwrap();
    assert!(enqueue_plan(&c, &plan, || panic!("missing prompt must not refresh")).is_err());
    assert!(!c.state_dir.join("jobs").exists());
}
#[test]
#[ignore = "set LAB_WORKFLOW_BIN for actual durable 32-task batch registration; no model launched"]
fn actual_workflow_batch_enqueue_is_idempotent_without_worker_launch() {
    let (_temp, c, plan) = batch_fixture(Some(std::env::var("LAB_WORKFLOW_BIN").unwrap().into()));
    verify_partial_batch_replay(&c, &plan);
    let w = Workflow::new(
        &c.workflow,
        c.state_dir.join("workflow.sqlite"),
        c.state_dir.join("workflow"),
    );
    for entry in &plan.tasks {
        let run = format!("{}-attempt-1", entry.task.id);
        assert_eq!(w.status(&run).unwrap()["status"], "running");
        w.verify(&run).unwrap();
    }
}
#[test]
fn frozen_v1_plan_remains_collectible_without_relabeling_or_relaxing_ids() {
    let (_temp, c, original, citation) = fixture_protocol(LEGACY_PROTOCOL);
    // Captured from the complete 32-task fixture at main 55c83f5, before v2.
    assert_eq!(
        digest(&original.tasks).unwrap(),
        "68b54f464bcd7f5280fd9c2dfc4d94090b78b8b54b6579904b617c8e374e75fe"
    );
    let plan: Plan = serde_json::from_slice(&serde_json::to_vec(&original).unwrap()).unwrap();
    validate_plan(&c, &plan).unwrap();
    let entry = &plan.tasks[0];
    let (job, receipt, state) = job_and_receipt(&c, &plan, entry, &card(&citation));
    storage::write(
        &c.state_dir
            .join("jobs")
            .join(format!("{}.json", entry.task.id)),
        &job,
    )
    .unwrap();
    storage::write(
        &c.state_dir
            .join("runs")
            .join(&job.run_id)
            .join("receipt.json"),
        &receipt,
    )
    .unwrap();
    let collected = collect(&c, &plan, None, |_| Ok(state.clone())).unwrap();
    assert_eq!(collected["protocol"], LEGACY_PROTOCOL);
    assert_eq!(collected["metrics"]["baseline"]["succeeded_calls"], 1);
    assert_eq!(
        collected["claims"][0]["id"],
        format!("{}#claim-one", entry.task.id)
    );
    let mut qualified = card(&citation);
    qualified.id = format!("{}#claim-one", entry.task.id);
    let (_, invalid, _) = job_and_receipt(&c, &plan, entry, &qualified);
    assert!(report_cards(entry, &invalid, 3, &BTreeSet::new()).is_err());

    let mut changed = plan.clone();
    changed.protocol = PROTOCOL.into();
    changed.digest = plan_digest(&changed).unwrap();
    assert!(
        validate_plan(&c, &changed).is_err(),
        "v1 tasks cannot be relabeled v2"
    );
    changed = plan.clone();
    changed.tasks[24]
        .task
        .dependencies
        .push(plan.tasks[0].task.id.clone());
    changed.digest = plan_digest(&changed).unwrap();
    assert!(
        validate_plan(&c, &changed).is_err(),
        "legacy replay still validates its DAG"
    );
    changed = plan;
    changed.protocol = "collaboration-pilot-v999".into();
    changed.digest = plan_digest(&changed).unwrap();
    assert!(
        validate_plan(&c, &changed).is_err(),
        "unknown protocols remain closed"
    );
}
#[test]
fn two_arms_have_matched_specialists_and_only_the_planned_peer_edges() {
    let (_temp, c, plan, _) = fixture();
    validate_plan(&c, &plan).unwrap();
    assert_eq!(plan.tasks.len(), 32);
    for i in 0..8 {
        let a = &plan.tasks[16 + i];
        let b = &plan.tasks[24 + i];
        assert_eq!(a.task.dependencies, vec![plan.tasks[i].task.id.clone()]);
        assert_eq!(
            b.task.dependencies,
            vec![
                plan.tasks[8 + i].task.id.clone(),
                plan.tasks[8 + (i + 1) % 8].task.id.clone(),
                plan.tasks[8 + (i + 2) % 8].task.id.clone()
            ]
        );
        assert_eq!(a.model, b.model);
        assert_eq!(a.task.prompt_version, b.task.prompt_version);
        assert_eq!(a.task.max_attempts, Some(1));
        assert_eq!(b.task.max_attempts, Some(1));
        assert!(!a.task.write && !a.task.use_memory && !b.task.write && !b.task.use_memory);
    }
    let mut tampered = plan.clone();
    tampered.tasks[24]
        .task
        .dependencies
        .push(plan.tasks[0].task.id.clone());
    tampered.digest = plan_digest(&tampered).unwrap();
    assert!(validate_plan(&c, &tampered).is_err());
    let mut request = plan.request.clone();
    request.slots[0].model = "different".into();
    assert!(validate_request(&c, &request).is_err());
}
#[test]
fn private_artifacts_are_immutable_and_symlink_escape_is_rejected() {
    let (temp, c, plan, _) = fixture();
    let root = c.state_dir.join("private/pilot");
    materialize(&c, &root, &plan).unwrap();
    materialize(&c, &root, &plan).unwrap();
    assert!(immutable_write(&c, &root.join("manifest.json"), &json!({"changed":true})).is_err());
    assert!(immutable_write(&c, &temp.path().join("public.json"), &plan).is_err());
    std::os::unix::fs::symlink(temp.path(), c.state_dir.join("private/escape")).unwrap();
    assert!(immutable_write(&c, &c.state_dir.join("private/escape/leak.json"), &plan).is_err());
}
#[test]
fn source_checks_require_exact_commit_digest_lines_and_quote() {
    let (_temp, _c, plan, citation) = fixture();
    check_citation(&plan, &citation).unwrap();
    for altered in [
        Citation {
            path: "../private/answers".into(),
            ..citation.clone()
        },
        Citation {
            commit: "b".repeat(40),
            ..citation.clone()
        },
        Citation {
            sha256: "c".repeat(64),
            ..citation.clone()
        },
        Citation {
            end_line: 999,
            ..citation.clone()
        },
        Citation {
            quote: "invented support".into(),
            ..citation
        },
    ] {
        assert!(check_citation(&plan, &altered).is_err());
    }
}
#[test]
fn receipt_authority_and_frozen_model_reject_local_tampering() {
    let (_temp, c, plan, citation) = fixture();
    let entry = &plan.tasks[0];
    let (job, receipt, state) = job_and_receipt(&c, &plan, entry, &card(&citation));
    check_receipt(&c, &plan, entry, &job, &state, &receipt).unwrap();
    let mut altered = receipt.clone();
    altered["agent_report"]["summary"] = json!("forged");
    assert!(check_receipt(&c, &plan, entry, &job, &state, &altered).is_err());
    let mut changed = job.clone();
    changed.model = "another model".into();
    assert!(check_receipt(&c, &plan, entry, &changed, &state, &receipt).is_err());
    changed = job;
    changed.attempt = 2;
    assert!(check_receipt(&c, &plan, entry, &changed, &state, &receipt).is_err());
}
#[test]
fn missing_runs_stay_incomplete_and_worker_changes_are_not_host_judgments() {
    let (_temp, c, plan, citation) = fixture();
    let empty = collect(&c, &plan, None, |_| panic!("no job should be queried")).unwrap();
    assert_eq!(empty["complete"], false);
    assert_eq!(empty["team_pairs"], 1);
    assert_eq!(empty["promotion_eligible"], false);
    let entry = &plan.tasks[0];
    let (job, receipt, state) = job_and_receipt(&c, &plan, entry, &card(&citation));
    storage::write(
        &c.state_dir
            .join("jobs")
            .join(format!("{}.json", entry.task.id)),
        &job,
    )
    .unwrap();
    storage::write(
        &c.state_dir
            .join("runs")
            .join(&job.run_id)
            .join("receipt.json"),
        &receipt,
    )
    .unwrap();
    let result = collect(&c, &plan, None, |_| Ok(state.clone())).unwrap();
    assert_eq!(result["claims"].as_array().unwrap().len(), 1);
    assert_eq!(result["fully_adjudicated"], false);
    assert_eq!(
        result["knowledge_proposals"][0]["status"],
        "pending_host_review"
    );
    assert_eq!(result["metrics"]["baseline"]["supported_claims"], 0);
}
#[test]
fn failed_calls_keep_usage_and_knowledge_directories_are_not_files() {
    let (_temp, c, plan, citation) = fixture();
    let entry = &plan.tasks[0];
    let (job, _, _) = job_and_receipt(&c, &plan, entry, &card(&citation));
    storage::write(
        &c.state_dir
            .join("jobs")
            .join(format!("{}.json", entry.task.id)),
        &job,
    )
    .unwrap();
    let dir = c.state_dir.join("runs").join(&job.run_id).join("process");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("stdout.jsonl"),
        "{\"type\":\"turn.completed\",\"usage\":{\"input_tokens\":17,\"output_tokens\":9}}\n",
    )
    .unwrap();
    let result = collect(&c, &plan, None, |_| Ok(json!({"status":"failed"}))).unwrap();
    assert_eq!(result["metrics"]["baseline"]["failed_calls"], 1);
    assert_eq!(result["tasks"][0]["usage"]["observed"]["input_tokens"], 17);
    assert_eq!(result["tasks"][1]["usage"]["observed"], Value::Null);
    let repo = &plan.inputs.repositories["superpod"];
    assert!(regular_source(&repo.checkout, &repo.upstream.commit, "knowledge").is_err());
    regular_source(&repo.checkout, &repo.upstream.commit, "knowledge/index.md").unwrap();
}
#[test]
fn cross_arm_references_and_stale_host_grades_are_rejected() {
    let (_temp, c, plan, citation) = fixture();
    let entry = &plan.tasks[24];
    let mut proposed = card(&citation);
    proposed.change = Change::Narrow;
    proposed.revises = vec![format!("{}#claim-one", plan.tasks[0].task.id)];
    let (_, receipt, _) = job_and_receipt(&c, &plan, entry, &proposed);
    let known = BTreeSet::from([proposed.revises[0].clone()]);
    assert!(report_cards(entry, &receipt, 3, &known).is_err());
    let initial = &plan.tasks[0];
    let (_, receipt, _) = job_and_receipt(&c, &plan, initial, &card(&citation));
    let mut claims = report_cards(initial, &receipt, 3, &BTreeSet::new()).unwrap();
    let mut grades = Adjudications {
        schema_version: 1,
        plan_digest: plan.digest.clone(),
        entries: vec![Adjudication {
            claim_id: claims[0].id.clone(),
            claim_digest: "stale".into(),
            supported: true,
            effective_counterexample: false,
            valid_revision: false,
            introduced_error: None,
            duplicate_of: None,
            rationale: "Independently checked exact source".into(),
            evidence: vec![citation],
        }],
    };
    assert!(apply_adjudications(&plan, &mut claims, &grades).is_err());
    grades.entries[0].claim_digest = claims[0].digest.clone();
    apply_adjudications(&plan, &mut claims, &grades).unwrap();
    assert!(claims[0].adjudication.as_ref().unwrap().supported);
    grades.entries[0].valid_revision = true;
    assert!(apply_adjudications(&plan, &mut claims, &grades).is_err());
}
