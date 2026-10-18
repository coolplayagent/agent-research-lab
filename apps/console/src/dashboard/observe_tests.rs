use super::*;
#[test]
fn open_stdout_writes_emit_session_events_before_close() {
    use std::io::Write;
    let path = std::env::temp_dir().join(format!(
        "observer-open-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&path).unwrap();
    let mut file = fs::File::create(path.join("stdout.jsonl")).unwrap();
    let mut watcher = Watcher::new().unwrap();
    watcher.add(&path, 0).unwrap();
    file.write_all(b"{\"type\":\"message\"}\n").unwrap();
    file.flush().unwrap();
    let (full, session, overflow) = watcher.receive(&path).unwrap();
    assert!(session);
    assert!(!full && !overflow);
    drop(file);
    fs::remove_dir_all(path).unwrap();
}
