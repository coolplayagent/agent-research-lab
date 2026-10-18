use super::*;
use crate::host_budget::{now, pause};
use crate::job_store::job_path;
use crate::tests::completion_fixture;
use crate::worker_identity::process_start;
use contracts::{ExecutionState, WorkflowNodeState, WorkflowState};
use native_effects::fs;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use task::{Job, Launch};

#[test]
fn controller_status_distinguishes_ready_blocked_dependencies_and_live_process() {
    let (_temp, _c, mut parent, _w) = completion_fixture(false, json!([]));
    let mut child = parent.clone();
    child.task.id = "waiting-child".into();
    child.task.dependencies = vec![parent.task.id.clone()];
    let ready = json!({"status":ExecutionState::Running,"frames":{"1":{"nodes":{"task":{"state":{"state":"task_ready"}}}}}});
    let mut states = BTreeMap::from([
        (
            parent.task.id.clone(),
            WorkflowStatusObservation::from_value(&ready),
        ),
        (
            child.task.id.clone(),
            WorkflowStatusObservation::from_value(&ready),
        ),
    ]);
    let classify = |job: &Job,
                    parent: &Job,
                    child: &Job,
                    states: &BTreeMap<String, WorkflowStatusObservation>| {
        let indexed = BTreeMap::from([
            (parent.task.id.clone(), parent),
            (child.task.id.clone(), child),
        ]);
        controller_state(job, &indexed, states, false, &mut BTreeSet::new())
    };
    assert_eq!(
        classify(&parent, &parent, &child, &states).0,
        ExecutionState::Ready
    );
    assert_eq!(
        classify(&child, &parent, &child, &states).0,
        ExecutionState::WaitingDependencies
    );
    parent.last_error = Some("latest baseline changed; enqueue a new experiment".into());
    assert_eq!(
        classify(&parent, &parent, &child, &states).0,
        ExecutionState::Blocked
    );
    let blocked = classify(&child, &parent, &child, &states);
    assert_eq!(blocked.0, ExecutionState::Blocked);
    assert!(blocked.1.unwrap().contains("latest baseline changed"));
    parent.last_error = None;
    parent.launch = Some(Launch {
        pid: std::process::id(),
        process_start: process_start(std::process::id()).unwrap(),
        lease: json!({}),
        attempt: json!({}),
        log_dir: PathBuf::new(),
        tools: json!({}),
        git_dir: String::new(),
        backend_request_sha256: None,
        communication: None,
        communication_gap: None,
        communication_member: None,
    });
    assert_eq!(
        classify(&parent, &parent, &child, &states).0,
        ExecutionState::Running
    );
    parent.launch.as_mut().unwrap().process_start = "not-the-retained-process".into();
    assert_eq!(
        classify(&parent, &parent, &child, &states).0,
        ExecutionState::NeedsReconciliation
    );
    parent.launch = None;
    states.insert(
        parent.task.id.clone(),
        WorkflowStatusObservation::from_value(&json!({"status":WorkflowState::Succeeded})),
    );
    assert_eq!(
        classify(&child, &parent, &child, &states).0,
        ExecutionState::Ready
    );
}

#[test]
fn summary_matches_detailed_controller_counts_without_losing_blockers() {
    let (_temp, c, base, w) = completion_fixture(false, json!([]));
    fs::remove_file(job_path(&c, &base.task.id)).unwrap();
    fs::write(
        &c.workflow,
        "#!/bin/sh\nset -eu\n[ \"$1 $2\" = 'run status' ]\ncat \"$4.json\"\n",
    )
    .unwrap();
    let ready = json!({"status":WorkflowState::Running,"frames":{"1":{"nodes":{"task":{"state":{"state":WorkflowNodeState::TaskReady}}}}}});
    let cases = [
        ("ready", Some(ready.clone()), vec![], ExecutionState::Ready),
        (
            "success",
            Some(json!({"status":WorkflowState::Succeeded})),
            vec![],
            ExecutionState::Succeeded,
        ),
        (
            "failed",
            Some(json!({"status":WorkflowState::Failed})),
            vec![],
            ExecutionState::Failed,
        ),
        (
            "cancelled",
            Some(json!({"status":WorkflowState::Cancelled})),
            vec![],
            ExecutionState::Failed,
        ),
        (
            "future",
            Some(json!({"status":"future_workflow_state"})),
            vec![],
            ExecutionState::Unknown,
        ),
        ("unavailable", None, vec![], ExecutionState::Unknown),
        (
            "waiting",
            Some(ready.clone()),
            vec!["ready"],
            ExecutionState::WaitingDependencies,
        ),
        (
            "blocked-failed",
            Some(ready.clone()),
            vec!["failed"],
            ExecutionState::Blocked,
        ),
        (
            "blocked-future",
            Some(ready.clone()),
            vec!["future"],
            ExecutionState::Blocked,
        ),
        (
            "blocked-unavailable",
            Some(ready.clone()),
            vec!["unavailable"],
            ExecutionState::Blocked,
        ),
        (
            "missing-dependency",
            Some(ready.clone()),
            vec!["absent"],
            ExecutionState::Blocked,
        ),
        (
            "cycle-a",
            Some(ready.clone()),
            vec!["cycle-b"],
            ExecutionState::Blocked,
        ),
        (
            "cycle-b",
            Some(ready.clone()),
            vec!["cycle-a"],
            ExecutionState::Blocked,
        ),
        (
            "workflow-paused",
            Some(json!({"status":WorkflowState::Running,"pause":{}})),
            vec![],
            ExecutionState::Paused,
        ),
        (
            "stale-launch",
            Some(ready.clone()),
            vec![],
            ExecutionState::NeedsReconciliation,
        ),
        (
            "live-launch",
            Some(ready.clone()),
            vec![],
            ExecutionState::Running,
        ),
        (
            "retry",
            Some(ready.clone()),
            vec![],
            ExecutionState::RetryWait,
        ),
        (
            "last-error",
            Some(ready.clone()),
            vec![],
            ExecutionState::Blocked,
        ),
        (
            "no-ready-task",
            Some(json!({"status":WorkflowState::Running})),
            vec![],
            ExecutionState::NeedsReconciliation,
        ),
        (
            "known-unknown",
            Some(json!({"status":WorkflowState::Unknown})),
            vec![],
            ExecutionState::NeedsReconciliation,
        ),
        (
            "missing-status",
            Some(json!({})),
            vec![],
            ExecutionState::Unknown,
        ),
    ];
    for (id, state, dependencies, _) in &cases {
        let mut job = base.clone();
        job.task.id = (*id).into();
        job.run_id = format!("{id}-attempt-1");
        job.task.dependencies = dependencies.iter().map(|id| (*id).into()).collect();
        if matches!(*id, "stale-launch" | "live-launch") {
            job.launch = Some(Launch {
                pid: std::process::id(),
                process_start: if *id == "live-launch" {
                    process_start(std::process::id()).unwrap()
                } else {
                    "different-process-identity".into()
                },
                lease: json!({}),
                attempt: json!({}),
                log_dir: PathBuf::new(),
                tools: json!({}),
                git_dir: String::new(),
                backend_request_sha256: None,
                communication: None,
                communication_gap: None,
                communication_member: None,
            });
        }
        if *id == "retry" {
            job.retry_after = Some(now() + 60);
        }
        if *id == "last-error" {
            job.last_error = Some("baseline needs review".into());
        }
        storage::write(&job_path(&c, id), &job).unwrap();
        if let Some(state) = state {
            storage::write(
                &w.work_dir.join(format!("{}.json", job.run_id)),
                &json!({"ok":true,"result":state}),
            )
            .unwrap();
        }
    }
    let budget = json!({"budget":{"days":{"1":17},"active_since":null},"main_seconds":17});
    storage::write(&c.state_dir.join("budget.json"), &budget).unwrap();
    for paused in [false, true] {
        pause(&c, paused).unwrap();
        let detailed = status(&c).unwrap();
        let summary = status_summary(&c).unwrap();
        let mut expected = BTreeMap::<ExecutionState, usize>::new();
        for (id, _, _, state) in &cases {
            let state = if paused && matches!(*id, "ready" | "no-ready-task" | "known-unknown") {
                ExecutionState::Paused
            } else {
                *state
            };
            let row = detailed["jobs"]
                .as_array()
                .unwrap()
                .iter()
                .find(|row| row["id"] == *id)
                .unwrap();
            assert_eq!(row["state"], json!(state), "{id}, paused={paused}");
            *expected.entry(state).or_default() += 1;
        }
        assert_eq!(detailed["summary"], json!(expected));
        assert_eq!(
            summary,
            json!({"paused":paused,"summary":expected,"total_jobs":cases.len(),"details_included":false,"budget":budget})
        );
        assert_eq!(detailed["budget"], summary["budget"]);
        assert_eq!(detailed["paused"], summary["paused"]);
        assert!(detailed.get("total_jobs").is_none());
        assert!(detailed.get("details_included").is_none());
        let unavailable = detailed["jobs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["id"] == "unavailable")
            .unwrap();
        assert!(unavailable["error"].as_str().is_some());
    }
}

#[test]
fn summary_omits_large_workflow_receipts_and_handles_an_empty_store() {
    let (temp, c, job, _w) = completion_fixture(false, json!([]));
    let path = temp.path().join("authority.json");
    let mut authority: Value = storage::read(&path).unwrap();
    let payload = "frozen-prompt-and-receipt-body".repeat(40000);
    authority["result"]["inputs"] = json!({"prompt":payload});
    authority["result"]["frames"]["1"]["nodes"]["task"]["outputs"]["result"] = json!(payload);
    storage::write(&path, &authority).unwrap();
    let detailed = status(&c).unwrap();
    let summary = status_summary(&c).unwrap();
    assert_eq!(summary["summary"], detailed["summary"]);
    assert_eq!(summary["summary"][ExecutionState::Succeeded.as_str()], 1);
    assert_eq!(detailed["jobs"][0]["status"], authority["result"]);
    assert!(serde_json::to_vec(&detailed).unwrap().len() > 2_000_000);
    let encoded = serde_json::to_string(&summary).unwrap();
    assert!(encoded.len() < 1024);
    assert!(!encoded.contains("frozen-prompt-and-receipt-body"));
    assert_eq!(summary.as_object().unwrap().len(), 5);
    assert!(summary.get("jobs").is_none());
    fs::remove_file(job_path(&c, &job.task.id)).unwrap();
    assert_eq!(
        status_summary(&c).unwrap(),
        json!({"paused":false,"summary":{},"total_jobs":0,"details_included":false,"budget":{}})
    );
}
