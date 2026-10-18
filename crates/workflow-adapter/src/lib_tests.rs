use super::*;
#[test]
fn bundle_identity_changes_with_prompts_and_effects() {
    let task = TaskSpec {
        id: "research".into(),
        payload: json!({"prompt":"A"}),
        write: false,
        timeout_ms: 60000,
    };
    let first = task_bundle(std::slice::from_ref(&task), false).unwrap();
    let mut changed = task.clone();
    changed.payload = json!({"prompt":"B"});
    assert_ne!(
        first["root"],
        task_bundle(&[changed], false).unwrap()["root"]
    );
    let mut write = task;
    write.write = true;
    let effect = task_bundle(&[write], false).unwrap();
    assert_eq!(
        effect["effect_bindings"][0]["policy"]["retry"]["max_calls"],
        1
    );
    assert_eq!(
        effect["capabilities"][0]["effects"]["idempotency"]["mode"],
        "none"
    );
}
#[test]
fn immutable_protocol_inputs_are_not_overwritten() {
    let temp = tempfile::tempdir().unwrap();
    let workflow = Workflow::new("unused", temp.path().join("db"), temp.path());
    let path = temp.path().join("request.json");
    workflow.persist_at(&path, &json!({"request":1})).unwrap();
    assert!(workflow.persist_at(&path, &json!({"request":2})).is_err());
    assert_eq!(fs::read_to_string(path).unwrap(), "{\"request\":1}");
}
#[test]
fn rejects_ambiguous_task_ids_and_empty_workflows() {
    assert!(task_bundle(&[], false).is_err());
    let task = TaskSpec {
        id: "done".into(),
        payload: json!({}),
        write: false,
        timeout_ms: 1,
    };
    assert!(task_bundle(&[task], false).is_err());
}
#[cfg(unix)]
#[test]
fn fake_transport_keeps_business_failure_and_rejects_invalid_json() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let binary = temp.path().join("fake-workflow");
    fs::write(
        &binary,
        "#!/bin/sh\nprintf '%s\\n' '{\"ok\":true,\"result\":{\"status\":\"failed\"}}'\n",
    )
    .unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
    let workflow = Workflow::new(&binary, temp.path().join("db"), temp.path());
    assert_eq!(
        status_name(&workflow.status("test").unwrap()).unwrap(),
        "failed"
    );
    fs::write(&binary, "#!/bin/sh\nprintf invalid-json\n").unwrap();
    assert!(workflow.status("test").is_err());
}
/// Explicit opt-in because this invokes an installed external CLI, never a simulated worker.
#[test]
#[ignore = "set LAB_WORKFLOW_BIN and run with --ignored for actual durable protocol integration"]
fn actual_runtime_read_write_pause_and_uncertainty() {
    let binary = std::env::var("LAB_WORKFLOW_BIN").expect("LAB_WORKFLOW_BIN");
    let temp = tempfile::tempdir().unwrap();
    let w = Workflow::new(binary, temp.path().join("runs.db"), temp.path());
    w.version().unwrap();
    w.initialize().unwrap();
    w.start_task("read", false, json!({"command":"/bin/true"}), 60000)
        .unwrap();
    let retained_path = temp.path().join(format!("start-{}.json", hex(b"read")));
    let retained_bytes = fs::read(&retained_path).unwrap();
    assert!(
        w.start_task("read", false, json!({"command":"/bin/true"}), 90000)
            .is_err()
    );
    assert!(
        w.replay_task_start("read", false, json!({"command":"/bin/true"}))
            .unwrap()
            .is_some()
    );
    assert_eq!(fs::read(&retained_path).unwrap(), retained_bytes);
    assert!(
        w.replay_task_start("read", true, json!({"command":"/bin/true"}))
            .is_err()
    );
    assert!(
        w.replay_task_start("read", false, json!({"command":"changed"}))
            .is_err()
    );
    let state = w.status("read").unwrap();
    w.pause(
        "read",
        "pause-1",
        state["revision"].as_u64().unwrap(),
        now_ms().unwrap(),
        "test pause",
    )
    .unwrap();
    let state = w.status("read").unwrap();
    w.resume(
        "read",
        "resume-1",
        state["revision"].as_u64().unwrap(),
        now_ms().unwrap(),
        "test resume",
    )
    .unwrap();
    let lease = w.acquire("read", "test", "read-1", 300000).unwrap();
    let claim = w.claim(&lease).unwrap();
    let output = process::capture("/bin/true", &[], temp.path(), Duration::from_secs(2)).unwrap();
    assert!(output.status.success());
    let settled = w
        .finish(
            &lease,
            &claim["attempt"],
            json!({"result":"actual /bin/true exited 0"}),
        )
        .unwrap();
    assert_eq!(settled["snapshot"]["status"], "succeeded");
    w.release(&lease).unwrap();
    w.verify("read").unwrap();
    w.start_task("write", true, json!({"output":"actual.txt"}), 60000)
        .unwrap();
    let lease = w.acquire("write", "test", "write-1", 300000).unwrap();
    let claim = w.effect_claim(&lease).unwrap();
    let path = temp.path().join("actual.txt");
    let output = process::capture(
        "/bin/sh",
        &[
            "-c".into(),
            "printf actual-write > \"$1\"".into(),
            "test".into(),
            path.to_string_lossy().into_owned(),
        ],
        temp.path(),
        Duration::from_secs(2),
    )
    .unwrap();
    assert!(output.status.success());
    let settled = w
        .finish_effect(
            &lease,
            &claim["attempt"],
            path.to_str().unwrap(),
            &path,
            json!({"result":"actual-write"}),
        )
        .unwrap();
    assert_eq!(settled["snapshot"]["status"], "succeeded");
    w.release(&lease).unwrap();
    w.verify("write").unwrap();
    w.start_task("uncertain", true, json!({"output":"partial.txt"}), 60000)
        .unwrap();
    let lease = w
        .acquire("uncertain", "test", "uncertain-1", 300000)
        .unwrap();
    let claim = w.effect_claim(&lease).unwrap();
    let output = process::capture(
        "/bin/sh",
        &["-c".into(), "printf partial > partial.txt; exit 7".into()],
        temp.path(),
        Duration::from_secs(2),
    )
    .unwrap();
    assert!(!output.status.success());
    w.unknown_effect(
        &lease,
        &claim["attempt"],
        "actual process exited 7 after writing partial.txt",
    )
    .unwrap();
    let next = w.effect_claim(&lease).unwrap();
    assert_ne!(next["type"], "call");
    assert_ne!(w.status("uncertain").unwrap()["status"], "succeeded");
    w.release(&lease).unwrap();
    w.verify("uncertain").unwrap();
    let tasks = [
        TaskSpec {
            id: "first".into(),
            payload: json!({}),
            write: false,
            timeout_ms: 1000,
        },
        TaskSpec {
            id: "second".into(),
            payload: json!({}),
            write: true,
            timeout_ms: 1000,
        },
    ];
    w.check_bundle(&task_bundle(&tasks, true).unwrap()).unwrap();
}
