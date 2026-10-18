use super::*;
use crate::job_store::job_path;
use crate::tests::completion_fixture;
use anyhow::{Result, bail, ensure};
use config::Config;
use native_effects::fs;
use serde_json::json;
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

#[test]
fn prerequisite_deadline_defers_without_rejecting_or_claiming_the_task() {
    let (temp, c, job, _) = completion_fixture(false, json!([]));
    let initial = fs::read(job_path(&c, &job.task.id)).unwrap();
    let mut ledger = Ledger::default();
    let window = RunWindow {
        started: now(),
        max_seconds: 30,
    };
    let started = std::time::Instant::now();
    let deferred = {
        let _deadline = process::deadline_scope(Duration::from_millis(80));
        prepare_prerequisite(&c, &mut ledger, window, true, || {
            process::capture(
                "/bin/sleep",
                &["2".into()],
                temp.path(),
                Duration::from_secs(2),
            )
        })
        .unwrap()
    };
    assert!(deferred.is_none());
    assert!(started.elapsed() < Duration::from_secs(1));
    assert_eq!(initial, fs::read(job_path(&c, &job.task.id)).unwrap());
    assert_eq!(
        prepare_prerequisite(&c, &mut ledger, window, false, || Ok(42)).unwrap(),
        Some(42),
        "an unclaimed task remains eligible after the transient deadline"
    );
    assert!(
        prepare_prerequisite::<()>(&c, &mut ledger, window, false, || {
            bail!("actual tool unavailable")
        })
        .is_err(),
        "a real capability failure must remain visible"
    );
}

fn config(root: &Path) -> Config {
    Config {
        schema_version: 1,
        workspace: root.into(),
        state_dir: root.join("state"),
        superpod: root.join("superpod"),
        codex: "unused".into(),
        multi_agent: None,
        agent_backends: BTreeMap::new(),
        role_backends: BTreeMap::new(),
        backend_models: BTreeMap::new(),
        workflow: root.join("unused"),
        daily_seconds: 43200,
        max_agents: 1,
        task_timeout_seconds: 60,
        models: Default::default(),
        tools: Default::default(),
        require_latest: false,
        skills_manifest: None,
    }
}
#[test]
fn idle_host_effects_respect_pause_and_exhausted_allowances() {
    let temp = tempfile::tempdir().unwrap();
    let c = config(temp.path());
    let mut ledger = load_ledger(&c).unwrap();
    let window = RunWindow {
        started: now(),
        max_seconds: 0,
    };
    pause(&c, true).unwrap();
    assert!(
        idle_host(&c, &mut ledger, window, || -> Result<()> {
            panic!("paused effect")
        })
        .unwrap()
        .is_none()
    );
    pause(&c, false).unwrap();
    ledger
        .budget
        .days
        .insert(budget::day(now()), c.daily_seconds);
    assert!(
        idle_host(&c, &mut ledger, window, || -> Result<()> {
            panic!("daily budget effect")
        })
        .unwrap()
        .is_none()
    );
    ledger.budget.days.clear();
    let expired = RunWindow {
        started: now() - 5,
        max_seconds: 1,
    };
    assert!(
        idle_host(&c, &mut ledger, expired, || -> Result<()> {
            panic!("expired effect")
        })
        .unwrap()
        .is_none()
    );
}
#[test]
fn idle_host_deadline_kills_slow_child_and_charges_failed_work() {
    let temp = tempfile::tempdir().unwrap();
    let c = config(temp.path());
    let mut ledger = load_ledger(&c).unwrap();
    let started = std::time::Instant::now();
    let result = idle_host(
        &c,
        &mut ledger,
        RunWindow {
            started: now(),
            max_seconds: 1,
        },
        || {
            let persisted: Ledger = storage::read(&c.state_dir.join("budget.json"))?;
            ensure!(
                persisted.budget.active_since.is_some(),
                "effect preceded budget journal"
            );
            process::capture(
                "/bin/sleep",
                &["10".into()],
                temp.path(),
                Duration::from_secs(30),
            )
        },
    );
    assert!(result.is_err());
    assert!(started.elapsed() < Duration::from_secs(4));
    let persisted: Ledger = storage::read(&c.state_dir.join("budget.json")).unwrap();
    assert!(persisted.budget.active_since.is_none());
    assert!(persisted.main_seconds >= 1);
    assert!(persisted.budget.days.values().sum::<u64>() >= 1);
}
