use super::*;
use std::io::Write;
#[test]
fn history_pages_preserve_public_events_across_byte_and_event_limits() {
    let root = std::env::temp_dir().join(format!(
        "lab-history-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(root.clone());
    for padding in [10, 12_000] {
        let path = root.join("runs/history/process/stdout.jsonl");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut log = File::create(&path).unwrap();
        for index in 0..125 {
            writeln!(log, "{}", json!({"type":"item.completed","item":{"type":"agent_message","text":format!("message-{index}:{}", include_str!("fixtures/character.txt").repeat(padding))}})).unwrap();
            writeln!(log, "{}", json!({"type":"item.completed","item":{"type":"reasoning","text":"PRIVATE_REASONING"}})).unwrap();
        }
        write!(log, "{{\"type\":\"item.completed\"").unwrap();
        drop(log);
        let mut before = None;
        let mut seen = BTreeMap::new();
        let mut pages = 0;
        loop {
            let current = page(&root, "history", before).unwrap();
            assert!(!current.to_string().contains("PRIVATE_REASONING"));
            let events = current["events"].as_array().unwrap();
            assert!(events.len() <= MAX_EVENTS);
            for event in events {
                let id = event["id"].as_u64().unwrap();
                assert!(
                    seen.insert(id, event["text"].as_str().unwrap().to_owned())
                        .is_none(),
                    "duplicate history event"
                );
            }
            pages += 1;
            assert!(pages < 100, "cursor must make bounded progress");
            let next = current["next_before"].as_u64();
            if let Some(next) = next {
                assert!(next < current["page_end"].as_u64().unwrap());
            } else {
                break;
            }
            before = next;
        }
        assert!(pages > 1);
        assert_eq!(seen.len(), 125);
        for (index, text) in seen.values().enumerate() {
            assert!(text.starts_with(&format!("message-{index}:")));
        }
        let empty = page(&root, "history", Some(0)).unwrap();
        assert!(empty["events"].as_array().unwrap().is_empty());
        assert!(empty["next_before"].is_null());
    }
}
#[test]
fn session_tail_is_bounded_tolerates_partial_lines_and_rejects_traversal() {
    let root = std::env::temp_dir().join(format!(
        "lab-observer-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(root.clone());
    let path = root.join("runs/test-attempt-1/process/stdout.jsonl");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut log = File::create(&path).unwrap();
    writeln!(
        log,
        "{}",
        json!({"type":"thread.started","thread_id":{"secret":"DO_NOT_PROJECT"}})
    )
    .unwrap();
    writeln!(log, "{}", "x".repeat(TAIL_BYTES as usize)).unwrap();
    for index in 0..60 {
        writeln!(log, "{}", json!({"type":"item.completed","item":{"type":"agent_message","text":format!("public-{index}")}})).unwrap();
    }
    writeln!(log, "invalid JSON").unwrap();
    write!(log, "{{\"type\":\"item.completed\"").unwrap();
    drop(log);
    let observed = read(&root, "test-attempt-1").unwrap();
    assert!(observed["session_id"].is_null());
    assert_eq!(observed["truncated"], true);
    assert_eq!(observed["invalid_lines"], 1);
    let events = observed["events"].as_array().unwrap();
    assert_eq!(events.len(), MAX_EVENTS);
    assert_eq!(events[0]["text"], "public-12");
    assert_eq!(events.last().unwrap()["text"], "public-59");
    assert!(
        events
            .windows(2)
            .all(|pair| pair[0]["id"].as_u64() < pair[1]["id"].as_u64())
    );
    for run in [
        "",
        "../test-attempt-1",
        "test/attempt",
        "%2e%2e",
        "test.attempt",
    ] {
        assert!(read(&root, run).is_err());
    }
}
#[test]
fn stdio_projection_keeps_public_activity_but_excludes_reasoning_and_credentials() {
    assert!(event(&json!({"type":"item.completed","item":{"type":"reasoning","text":"PRIVATE_REASONING"}}),0).is_none());
    let projected=event(&json!({"type":"item.completed","item":{"type":"command_execution","command":"curl -H 'Authorization: Bearer SECRET'","aggregated_output":"done","status":"completed","exit_code":0},"lease":"PRIVATE_LEASE"}),45).unwrap();
    assert_eq!(projected["id"], 45);
    assert_eq!(projected["exit_code"], 0);
    assert_eq!(projected["output"], "done");
    assert!(!projected.to_string().contains("SECRET"));
    assert!(!projected.to_string().contains("PRIVATE_LEASE"));
    assert_eq!(
            event(
                &json!({"type":"item.completed","item":{"type":"agent_message","text":"Public progress"}}),
                0
            )
            .unwrap()["text"],
            "Public progress"
        );
}
