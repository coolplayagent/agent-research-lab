use super::*;
use std::os::unix::fs::PermissionsExt;

#[test]
fn historical_start_survives_timeout_default_change_without_rebinding() {
    let temp = tempfile::tempdir().unwrap();
    let binary = temp.path().join("workflow-fixture");
    fs::write(
        &binary,
        "#!/bin/sh\nprintf '%s\\n' '{\"ok\":true,\"result\":{}}'\n",
    )
    .unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
    let mut config: Config = serde_json::from_value(json!({
        "schema_version":1,"workspace":temp.path(),"state_dir":temp.path().join("state"),
        "superpod":temp.path(),"codex":"unused","workflow":binary,
        "daily_seconds":3600,"max_agents":1,"task_timeout_seconds":20,
        "models":{},"tools":{},"require_latest":false,"skills_manifest":null
    }))
    .unwrap();
    let mut job: Job = serde_json::from_value(json!({
        "task":{"id":"retained","role":"research","repository":"superpod","prompt":"frozen"},
        "model":"fixture","source_commit":"source","superpod_commit":"knowledge",
        "prompt_digest":"prompt","config_digest":"config","worktree":temp.path(),
        "run_id":"retained-attempt-1","attempt":1,"last_error":null,"launch":null
    }))
    .unwrap();
    ensure_started(&config, &job).unwrap();
    let retained_path = workflow(&config).work_dir.join(format!(
        "start-{}.json",
        storage::digest(job.run_id.as_bytes())
    ));
    let original = fs::read(&retained_path).unwrap();
    config.task_timeout_seconds = 90;
    ensure_started(&config, &job).unwrap();
    assert_eq!(fs::read(&retained_path).unwrap(), original);
    let retained: Value = storage::read(&retained_path).unwrap();
    assert_eq!(retained["bundle"]["capabilities"][0]["timeout_ms"], 20000);
    job.task.prompt = "changed".into();
    assert!(ensure_started(&config, &job).is_err());
    assert_eq!(fs::read(&retained_path).unwrap(), original);
    job.task.prompt = "frozen".into();
    fs::write(&retained_path, b"not-json").unwrap();
    assert!(ensure_started(&config, &job).is_err());
    assert_eq!(fs::read(&retained_path).unwrap(), b"not-json");
}
