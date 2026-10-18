use super::*;
use crate::job_store::job_path;
use crate::tests::completion_fixture;
use crate::worker_identity::process_start;
use process::Process;
use serde_json::json;
use std::path::PathBuf;
use std::time::Duration;
use task::Launch;

#[test]
fn retained_worker_cleanup_uses_start_identity_without_cli_or_budget() {
    let (temp, mut c, mut job, _w) = completion_fixture(false, json!([]));
    c.workflow = PathBuf::from("/bin/false");
    storage::write(&c.state_dir.join("paused"), &json!({"paused":true})).unwrap();
    let mut process = Process::spawn(
        "/bin/sleep",
        &["30".into()],
        temp.path(),
        &temp.path().join("sleep-logs"),
        Duration::from_secs(30),
        &[],
        None,
    )
    .unwrap();
    let start = process_start(process.pid()).unwrap();
    job.launch = Some(Launch {
        pid: process.pid(),
        process_start: format!("{start}-mismatch"),
        lease: json!({}),
        attempt: json!({}),
        log_dir: temp.path().join("sleep-logs"),
        tools: json!({}),
        git_dir: String::new(),
        backend_request_sha256: None,
        communication: None,
        communication_gap: None,
        communication_member: None,
    });
    storage::write(&job_path(&c, &job.task.id), &job).unwrap();
    kill_retained_workers(&c).unwrap();
    assert!(process.poll().unwrap().is_none());
    job.launch.as_mut().unwrap().process_start = start;
    storage::write(&job_path(&c, &job.task.id), &job).unwrap();
    kill_retained_workers(&c).unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    loop {
        if let Some(status) = process.poll().unwrap() {
            assert!(!status.success());
            break;
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
}
