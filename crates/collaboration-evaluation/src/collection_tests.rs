use super::*;
use crate::{
    planning::plan_digest,
    protocol::{LEGACY_PROTOCOL, PROTOCOL},
    tests::{card, fixture, fixture_protocol, job_and_receipt},
};

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
fn workflow_observations_preserve_wire_states_and_never_guess_success() {
    let (_temp, c, plan, citation) = fixture();
    let entry = &plan.tasks[0];
    let (job, receipt, committed) = job_and_receipt(&c, &plan, entry, &card(&citation));
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
    let mut cases: Vec<_> = WorkflowState::ALL
        .iter()
        .map(|state| {
            (
                serde_json::to_value(state).unwrap(),
                *state == WorkflowState::Succeeded,
                matches!(state, WorkflowState::Failed | WorkflowState::Cancelled),
            )
        })
        .collect();
    cases.extend([
        (json!("canceled"), false, true),
        (json!("timed_out"), false, true),
        (json!("future_terminal"), false, false),
        (json!("Succeeded"), false, false),
        (json!("succeeded "), false, false),
        (json!(""), false, false),
        (Value::Null, false, false),
        (json!(true), false, false),
        (json!({"state":"succeeded"}), false, false),
    ]);
    for (wire, succeeded, failed) in cases {
        let mut observation = committed.clone();
        observation["status"] = wire.clone();
        let result = collect(&c, &plan, None, |_| Ok(observation.clone())).unwrap();
        let metric = &result["metrics"]["baseline"];
        assert_eq!(metric["scheduled_calls"], 16);
        assert_eq!(metric["succeeded_calls"], usize::from(succeeded));
        assert_eq!(metric["failed_calls"], usize::from(failed));
        assert_eq!(
            metric["incomplete_calls"],
            if succeeded || failed { 15 } else { 16 }
        );
        assert_eq!(metric["invalid_reports"], 0);
        assert_eq!(
            result["claims"].as_array().unwrap().len(),
            usize::from(succeeded)
        );
        assert_eq!(
            result["tasks"][0]["state"],
            wire.as_str().unwrap_or(WorkflowState::Unknown.as_str())
        );
        assert_eq!(result["complete"], false);
        assert_eq!(result["promotion_eligible"], false);
    }
    let mut missing = committed;
    missing.as_object_mut().unwrap().remove("status");
    let result = collect(&c, &plan, None, |_| Ok(missing.clone())).unwrap();
    assert_eq!(result["metrics"]["baseline"]["incomplete_calls"], 16);
    assert_eq!(result["tasks"][0]["state"], WorkflowState::Unknown.as_str());
    assert!(result["claims"].as_array().unwrap().is_empty());
}
