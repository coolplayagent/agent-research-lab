use super::*;
use crate::{
    host_budget::now,
    job_store::{job_path, workflow},
    launch::launch_guarded,
    post_success::post_success_path,
    tests::admission_tests::{DirectAgentFixture, Hook, events, fixture},
};
use native_effects::fs;
use serde_json::Value;
use std::time::{Duration, Instant};

fn active_fixture(mode: &str) -> (tempfile::TempDir, Config, Job, Workflow, Workers, Hook) {
    let _direct_agent = DirectAgentFixture::enter();
    let (temp, config, mut job, _) = fixture(mode);
    // A harmless process stays alive long enough for the production PID join.
    fs::write(&config.codex, "#!/bin/sh\nsleep 0.2\nexit 99\n").unwrap();
    let owner = workflow(&config);
    let mut hook = Hook {
        allow_model_entry: true,
        ..Hook::default()
    };
    let mut writes = admission::JobWrites::new(&job, true);
    let (process, heartbeat) = launch_guarded(
        &config,
        &owner,
        &mut job,
        10,
        &mut Some(&mut hook),
        &mut writes,
    )
    .unwrap();
    let mut workers = Workers::default();
    workers.push(job.clone(), writes, process, heartbeat);
    (temp, config, job, owner, workers, hook)
}

fn drain(workers: &mut Workers, config: &Config, owner: &Workflow, hook: &mut Hook) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut failed = false;
    while !workers.is_empty() {
        failed |= workers
            .poll(config, owner, false, now(), &mut Some(hook))
            .unwrap();
        assert!(
            Instant::now() < deadline,
            "worker did not settle within its bound"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    failed
}

#[test]
fn guarded_worker_failure_releases_once_and_disables_automatic_retry() {
    let (temp, config, job, owner, mut workers, mut hook) = active_fixture("worker-exit");
    assert!(drain(&mut workers, &config, &owner, &mut hook));
    assert!(hook.stopped);
    let current: Job = storage::read(&job_path(&config, &job.task.id)).unwrap();
    assert_eq!(current.attempt, job.attempt);
    assert!(current.launch.is_none());
    assert!(current.retry_after.is_none());
    assert_eq!(
        current.last_error.as_deref(),
        Some("worker exited Some(99)")
    );
    let observed: Value = storage::read(
        &owner
            .work_dir
            .join("host-exits")
            .join(format!("{}.json", job.run_id)),
    )
    .unwrap();
    assert_eq!(observed["success"], false);
    assert_eq!(observed["code"], 99);
    let effects: Vec<_> = events(&temp)
        .into_iter()
        .filter(|event| event == "finish" || event == "release")
        .collect();
    assert_eq!(effects, ["finish", "release"]);
    assert!(!post_success_path(&config, &job).exists());
}

#[test]
fn active_attempt_drift_preserves_new_job_and_never_publishes_old_completion() {
    let (temp, config, job, owner, mut workers, mut hook) = active_fixture("worker-exit");
    let mut replacement = job.clone();
    replacement.attempt += 1;
    replacement.run_id = format!("{}-attempt-{}", job.task.id, replacement.attempt);
    replacement.launch = None;
    replacement.last_error = Some("independent current attempt".into());
    storage::write(&job_path(&config, &job.task.id), &replacement).unwrap();
    let expected = fs::read(job_path(&config, &job.task.id)).unwrap();
    let error = workers
        .poll(&config, &owner, true, now(), &mut Some(&mut hook))
        .unwrap_err();
    assert!(error.to_string().contains("frozen job authority changed"));
    assert!(hook.stopped);
    assert_eq!(fs::read(job_path(&config, &job.task.id)).unwrap(), expected);
    assert!(
        !owner
            .work_dir
            .join("host-exits")
            .join(format!("{}.json", job.run_id))
            .exists()
    );
    let effects: Vec<_> = events(&temp)
        .into_iter()
        .filter(|event| event == "finish" || event == "release")
        .collect();
    assert_eq!(effects, ["release"]);
}

#[test]
fn ambiguous_settlement_retains_original_launch_for_reconciliation() {
    let (temp, config, job, owner, mut workers, mut hook) = active_fixture("finish-error");
    assert!(drain(&mut workers, &config, &owner, &mut hook));
    assert!(hook.stopped);
    let current: Job = storage::read(&job_path(&config, &job.task.id)).unwrap();
    let retained = current.launch.as_ref().unwrap();
    let original = job.launch.as_ref().unwrap();
    assert_eq!(retained.pid, original.pid);
    assert_eq!(retained.process_start, original.process_start);
    assert_eq!(retained.attempt, original.attempt);
    assert_eq!(retained.lease["run_id"], job.run_id);
    assert!(current.retry_after.is_none());
    assert!(
        current
            .last_error
            .as_deref()
            .unwrap()
            .contains("settlement requires reconciliation")
    );
    let effects: Vec<_> = events(&temp)
        .into_iter()
        .filter(|event| event == "finish" || event == "release")
        .collect();
    assert_eq!(effects, ["finish", "release"]);
    assert!(!post_success_path(&config, &job).exists());
}
