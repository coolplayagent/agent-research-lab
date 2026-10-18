use super::*;
use crate::{identity::digest, planning::plan_digest, tests::fixture};
use std::{fs, path::PathBuf};
use workflow::Workflow;

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
