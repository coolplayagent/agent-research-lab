use super::*;
#[test]
fn failed_spawn_removes_its_private_log_directory() {
    let root = tempfile::tempdir().unwrap();
    let config = service_api::Configuration::defaults(root.path());
    assert!(
        Child::spawn(
            &root.path().join("absent-worker"),
            &config.services[0],
            root.path()
        )
        .is_err()
    );
    let directory = root
        .path()
        .join(format!("private/managed-{}", config.services[0].id));
    assert_eq!(std::fs::read_dir(directory).unwrap().count(), 0);
}
#[cfg(unix)]
#[test]
fn owned_process_is_killed_and_private_logs_removed_on_drop() {
    let root = tempfile::tempdir().unwrap();
    let logs = root.path().join("launch");
    let process = process::Process::spawn(
        "/bin/sleep",
        &["30".into()],
        root.path(),
        &logs,
        std::time::Duration::from_secs(1),
        &[],
        None,
    )
    .unwrap();
    let pid = process.pid();
    let mut child = Child {
        process: Some(process),
        logs: logs.clone(),
    };
    assert!(child.running());
    drop(child);
    assert!(!Path::new(&format!("/proc/{pid}")).exists());
    assert!(!logs.exists());
}
