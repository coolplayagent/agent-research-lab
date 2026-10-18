use super::*;
use crate::{jobs, reconcile_orphans, tests::completion_fixture};
use std::time::Duration;
use task::Launch;

fn drain_followups(c: &Config, w: &Workflow) -> Result<bool> {
    drain_followups_scoped(c, w, None, &mut BTreeSet::new())
}

#[test]
fn completion_recovery_reconstructs_missing_journal_and_remembers_once() {
    let (temp, c, mut job, w) = completion_fixture(true, json!([]));
    job.launch = Some(Launch {
        pid: 0,
        process_start: String::new(),
        lease: json!({"run_id":job.run_id}),
        attempt: json!({}),
        log_dir: c.state_dir.join("runs").join(&job.run_id),
        tools: json!({}),
        git_dir: String::new(),
        backend_request_sha256: None,
        communication: None,
        communication_gap: None,
        communication_member: None,
    });
    storage::write(&job_path(&c, &job.task.id), &job).unwrap();
    // Neither a forged log receipt nor a forged memory success is authority.
    storage::write(
        &c.state_dir
            .join("runs")
            .join(&job.run_id)
            .join("receipt.json"),
        &json!({"forged":true}),
    )
    .unwrap();
    storage::write(
        &c.state_dir
            .join("runs")
            .join(&job.run_id)
            .join("memory-remember.json"),
        &json!({"forged":true}),
    )
    .unwrap();
    reconcile_orphans(&c, &w).unwrap();
    let recovered: Job = storage::read(&job_path(&c, &job.task.id)).unwrap();
    assert!(recovered.launch.is_none());
    let record: PostSuccess = storage::read(&post_success_path(&c, &job)).unwrap();
    assert_eq!(record.memory, MemoryWriteback::Pending);
    assert!(drain_followups(&c, &w).unwrap());
    assert_eq!(fs::read(temp.path().join("memory-calls")).unwrap(), b"x");
    reconcile_orphans(&c, &w).unwrap();
    assert!(!drain_followups(&c, &w).unwrap());
    assert_eq!(fs::read(temp.path().join("memory-calls")).unwrap(), b"x");
    assert_eq!(w.status(&job.run_id).unwrap()["status"], "succeeded");
}

#[test]
fn completion_recovery_never_repeats_unknown_memory_write() {
    let (temp, c, job, w) = completion_fixture(true, json!([]));
    queue_post_success(&c, &w, &job).unwrap();
    let path = post_success_path(&c, &job);
    let mut record: PostSuccess = storage::read(&path).unwrap();
    record.memory = MemoryWriteback::Running;
    storage::write(&path, &record).unwrap();
    fs::write(temp.path().join("memory-calls"), "already-applied").unwrap();
    reconcile_orphans(&c, &w).unwrap();
    let recovered: PostSuccess = storage::read(&path).unwrap();
    assert_eq!(recovered.memory, MemoryWriteback::Unknown);
    assert!(drain_followups(&c, &w).unwrap());
    assert_eq!(
        fs::read_to_string(temp.path().join("memory-calls")).unwrap(),
        "already-applied"
    );
    let recovered: Job = storage::read(&job_path(&c, &job.task.id)).unwrap();
    assert!(
        recovered
            .last_error
            .unwrap()
            .contains("memory writeback gap")
    );
    assert_eq!(w.status(&job.run_id).unwrap()["status"], "succeeded");
}

#[test]
fn completion_recovery_replays_followup_admission_without_duplicate_children() {
    let (_temp, c, job, w) = completion_fixture(
        false,
        json!([{
            "repository":"superpod","role":"review","prompt":"inspect actual frozen evidence",
            "write":false,"exploratory":false
        }]),
    );
    queue_post_success(&c, &w, &job).unwrap();
    let receipt = authoritative_success_receipt(&w, &job).unwrap();
    let dir = c.state_dir.join("runs").join(&job.run_id);
    admit_followups(&c, &job, &dir, &receipt).unwrap();
    let path = post_success_path(&c, &job);
    let mut record: PostSuccess = storage::read(&path).unwrap();
    record.followups = FollowupAdmission::Running;
    storage::write(&path, &record).unwrap();
    assert_eq!(jobs(&c).unwrap().len(), 2);
    assert!(drain_followups(&c, &w).unwrap());
    assert_eq!(jobs(&c).unwrap().len(), 2);
    let record: PostSuccess = storage::read(&path).unwrap();
    assert_eq!(record.followups, FollowupAdmission::Done);
    assert!(!drain_followups(&c, &w).unwrap());
}

#[test]
fn mixed_followups_keep_original_indices_and_replay_without_role_escalation() {
    let (_temp, c, job, w) = completion_fixture(
        false,
        json!([
            {"repository":"superpod","role":"research","prompt":"requires a write","write":true,"exploratory":false},
            {"repository":"superpod","role":"review","prompt":"inspect evidence","write":false,"exploratory":false},
            {"repository":"superpod","role":"review","prompt":"malformed missing fields"}
        ]),
    );
    queue_post_success(&c, &w, &job).unwrap();
    assert!(drain_followups(&c, &w).unwrap());
    let path = post_success_path(&c, &job);
    let mut record: PostSuccess = storage::read(&path).unwrap();
    assert_eq!(record.followups, FollowupAdmission::Done);
    assert_eq!(
        record
            .followup_rejections
            .iter()
            .map(|r| r.index)
            .collect::<Vec<_>>(),
        vec![0, 2]
    );
    let expected = format!(
        "followup-{}-1",
        &storage::digest(job.run_id.as_bytes())[..16]
    );
    assert_eq!(record.followup_queued, vec![expected.clone()]);
    let child: Job = storage::read(&job_path(&c, &expected)).unwrap();
    assert_eq!(child.task.role, "review");
    assert!(!child.task.write);
    assert!(
        child.task.required_tools.is_empty(),
        "legacy proposals default to no tool requirement"
    );
    assert_eq!(jobs(&c).unwrap().len(), 2);
    // A crash after child creation but before parent-journal settlement
    // cannot renumber the child or discard the invalid proposal's reason.
    record.followups = FollowupAdmission::Running;
    record.followup_queued.clear();
    record.followup_rejections.clear();
    storage::write(&path, &record).unwrap();
    assert!(drain_followups(&c, &w).unwrap());
    let replayed: PostSuccess = storage::read(&path).unwrap();
    assert_eq!(replayed.followup_queued, vec![expected]);
    assert_eq!(replayed.followup_rejections.len(), 2);
    assert_eq!(jobs(&c).unwrap().len(), 2);
}

#[test]
fn unmigrated_followup_journal_is_rejected_without_automatic_replay() {
    let (_temp, c, job, w) = completion_fixture(false, json!([]));
    queue_post_success(&c, &w, &job).unwrap();
    let path = post_success_path(&c, &job);
    let mut old: Value = storage::read(&path).unwrap();
    old.as_object_mut().unwrap().remove("followup_queued");
    old.as_object_mut().unwrap().remove("followup_rejections");
    old["followups"] = json!("blocked");
    old["followup_error"] = json!("historical proposal requires host review");
    storage::write(&path, &old).unwrap();
    assert!(storage::read::<PostSuccess>(&path).is_err());
    assert!(queue_post_success(&c, &w, &job).is_err());
    assert_eq!(storage::read::<Value>(&path).unwrap(), old);
}

#[test]
fn startup_rejects_retired_completion_before_execution() {
    let (_temp, c, job, _w) = completion_fixture(false, json!([]));
    let path = post_success_path(&c, &job);
    storage::write(&path, &json!({"task_id":job.task.id,"run_id":job.run_id})).unwrap();
    let before = fs::read(&path).unwrap();
    assert!(validate_state(&c).is_err());
    assert_eq!(fs::read(&path).unwrap(), before);
}

#[test]
fn completion_recovery_defers_budget_exhaustion_then_reuses_partial_child() {
    let (_temp, c, job, w) = completion_fixture(
        false,
        json!([{
            "repository":"superpod","role":"review","prompt":"bounded follow-up",
            "write":false,"exploratory":false
        }]),
    );
    queue_post_success(&c, &w, &job).unwrap();
    let original = fs::read_to_string(&c.workflow).unwrap();
    let slow = original.replacen(
        "#!/bin/sh\n",
        "#!/bin/sh\nif [ \"$1\" = run ] && [ \"$2\" = init ]; then sleep 5; fi\n",
        1,
    );
    fs::write(&c.workflow, slow).unwrap();
    {
        let _deadline = process::deadline_scope(Duration::from_millis(300));
        assert!(drain_followups(&c, &w).unwrap());
        assert!(process::deadline_exhausted());
    }
    let record: PostSuccess = storage::read(&post_success_path(&c, &job)).unwrap();
    assert_eq!(record.followups, FollowupAdmission::Pending);
    assert_eq!(jobs(&c).unwrap().len(), 2);
    fs::write(&c.workflow, original).unwrap();
    assert!(drain_followups(&c, &w).unwrap());
    let record: PostSuccess = storage::read(&post_success_path(&c, &job)).unwrap();
    assert_eq!(record.followups, FollowupAdmission::Done);
    assert!(record.followup_error.is_none());
    assert_eq!(jobs(&c).unwrap().len(), 2);
}

#[test]
fn completion_recovery_rejects_unmigrated_journal() {
    let (temp, c, job, w) = completion_fixture(true, json!([]));
    let authority = temp.path().join("authority.json");
    let mut state: Value = storage::read(&authority).unwrap();
    let output = &mut state["result"]["frames"]["1"]["nodes"]["task"]["outputs"]["result"];
    let mut receipt: Value = serde_json::from_str(output.as_str().unwrap()).unwrap();
    receipt
        .as_object_mut()
        .unwrap()
        .remove("post_success_protocol");
    *output = json!(serde_json::to_string(&receipt).unwrap());
    storage::write(&authority, &state).unwrap();
    assert!(reconcile_orphans(&c, &w).is_err());
    assert!(queue_post_success(&c, &w, &job).is_err());
    assert!(!post_success_path(&c, &job).exists());
    assert!(!temp.path().join("memory-calls").exists());
}
