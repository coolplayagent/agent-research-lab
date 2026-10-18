use super::*;
use std::{fs, path::PathBuf};

#[cfg(unix)]
#[test]
fn index_refresh_respects_active_work_and_requires_complete_exact_snapshot() {
    let temp = tempfile::tempdir().unwrap();
    let fixture = if let Some(runfiles) = std::env::var_os("TEST_SRCDIR") {
        PathBuf::from(runfiles)
            .join(std::env::var_os("TEST_WORKSPACE").expect("Bazel test workspace"))
            .join("crates/knowledge-publication/fixtures/index-refresh.sh")
    } else {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/index-refresh.sh")
    };
    let expected = "a".repeat(40);
    let request = IndexRefreshRequest {
        relay_knowledge_binary: fixture.canonicalize().unwrap(),
        knowledge_root: temp.path().into(),
        repository_alias: "superpod".into(),
        expected_commit: expected.clone(),
    };
    #[cfg(target_os = "linux")]
    let _private_script_writer = {
        use std::os::unix::fs::PermissionsExt;
        let binary = temp.path().join("relay-knowledge-fixture");
        fs::copy(&request.relay_knowledge_binary, &binary).unwrap();
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
        let writer = fs::OpenOptions::new().write(true).open(&binary).unwrap();
        let mut busy_request = request.clone();
        busy_request.relay_knowledge_binary = binary;
        let error = refresh_index(&busy_request).unwrap_err();
        let io_error = error
            .chain()
            .find_map(|cause| cause.downcast_ref::<std::io::Error>())
            .expect("the writable executable must fail with an I/O error");
        assert_eq!(io_error.raw_os_error(), Some(26));
        writer
    };
    let complete = serde_json::json!({"status":{"last_indexed_commit":expected,"stale":false,"content_integrity":{"state":"complete"}},"retention":{"maintenance_pending":false}});
    let old = serde_json::json!({"status":{"last_indexed_commit":"b".repeat(40),"stale":true,"content_integrity":{"state":"complete"}}});
    let mut cases = Vec::new();
    let mut active = old.clone();
    active["active_task"] = serde_json::json!({"state":"running"});
    cases.push(("top-level active task", active, complete.clone(), false, 0));
    let mut nested = old.clone();
    nested["status"]["active_task"] = serde_json::json!({"state":"queued"});
    cases.push(("legacy nested task", nested, complete.clone(), false, 0));
    let mut maintenance = old.clone();
    maintenance["retention"] = serde_json::json!({"maintenance_pending":true});
    cases.push((
        "maintenance in progress",
        maintenance,
        complete.clone(),
        false,
        0,
    ));
    let mut incomplete = complete.clone();
    incomplete["status"]["content_integrity"]["state"] = serde_json::json!("incomplete");
    cases.push(("incomplete bytes", incomplete.clone(), incomplete, false, 1));
    let missing = serde_json::json!({"status":{"last_indexed_commit":expected,"stale":false}});
    cases.push((
        "missing integrity proof",
        missing.clone(),
        missing,
        false,
        1,
    ));
    let mut conflicting = complete.clone();
    conflicting["status"]["content_integrity"] =
        serde_json::json!({"state":"degraded","complete":true});
    cases.push((
        "contradictory integrity proof",
        conflicting.clone(),
        conflicting,
        false,
        1,
    ));
    cases.push((
        "already complete",
        complete.clone(),
        complete.clone(),
        true,
        0,
    ));
    cases.push(("one bounded update reaches target", old, complete, true, 1));
    for (name, before, after, ready, updates) in cases {
        fs::write(
            temp.path().join("status.json"),
            serde_json::to_vec(&before).unwrap(),
        )
        .unwrap();
        fs::write(
            temp.path().join("after.json"),
            serde_json::to_vec(&after).unwrap(),
        )
        .unwrap();
        fs::write(temp.path().join("updates.log"), "").unwrap();
        let result = refresh_index(&request).unwrap();
        assert_eq!(result.ready, ready, "{name}");
        assert_eq!(
            fs::read_to_string(temp.path().join("updates.log"))
                .unwrap()
                .lines()
                .count(),
            updates,
            "{name}"
        );
    }
}

#[test]
fn integrity_protocol_never_promotes_unknown_states_or_missing_freshness() {
    let expected = "a".repeat(40);
    let legacy = serde_json::json!({"last_indexed_commit":expected,"stale":false,"content_integrity":{"complete":true}});
    assert!(ready(&legacy, &expected));
    assert!(!ready(&legacy, &"b".repeat(40)));
    for state in [
        serde_json::json!("future_state"),
        serde_json::json!("degraded"),
        serde_json::Value::Null,
        serde_json::json!(true),
    ] {
        let mut value = legacy.clone();
        value["content_integrity"]["state"] = state;
        assert!(!ready(&value, &expected));
        assert!(!ready(&serde_json::json!({"status":value}), &expected));
    }
    for stale in [
        serde_json::Value::Null,
        serde_json::json!("false"),
        serde_json::json!(true),
    ] {
        let mut value = legacy.clone();
        value["stale"] = stale;
        assert!(!ready(&value, &expected));
    }
}
