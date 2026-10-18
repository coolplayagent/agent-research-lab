use super::*;
use crate::{job_store::job_path, tests::completion_fixture};
use multi_agent::TeamMembership;
use task::Job;

#[test]
fn partial_communication_registration_recovers_from_private_launch_intent() {
    let (_temp, c, mut job, _w) = completion_fixture(false, json!([]));
    let team = multi_agent::TeamBinding {
        id: "intent-recovery".into(),
        cell: None,
        topics: vec!["sdlc".into()],
        query: String::new(),
    };
    job.task.communication = Some(team.clone());
    let member = team.member(&job).unwrap();
    let board = communication::Board::new(&c.state_dir).unwrap();
    let time = now().max(0) as u64;
    // The host saved intent, then registered, then crashed before receiving context.
    job.launch = Some(serde_json::from_value(json!({"pid":0,"process_start":"","lease":{},"attempt":{},"log_dir":c.state_dir.join("runs").join(&job.run_id),"communication_member":member})).unwrap());
    storage::write(&job_path(&c, &job.task.id), &job).unwrap();
    board.register(&member, time).unwrap();
    let recovered: Job = storage::read(&job_path(&c, &job.task.id)).unwrap();
    assert!(recovered.launch.as_ref().unwrap().communication.is_none());
    revoke_communication(&c, &recovered, recovered.launch.as_ref().unwrap());
    let first = board.view(&member.cohort_id, now().max(0) as u64).unwrap();
    assert_eq!(first["members"][0]["origin"]["state"], "revoked");
    revoke_communication(&c, &recovered, recovered.launch.as_ref().unwrap());
    assert_eq!(
        board.view(&member.cohort_id, now().max(0) as u64).unwrap()["revision"],
        first["revision"]
    );
}
