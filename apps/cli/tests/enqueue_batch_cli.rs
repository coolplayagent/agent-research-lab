//! Exercise actual CLI envelopes and native collect using owned acquisition boundaries.
#[test]
#[ignore = "requires LAB_CLI_BIN and LAB_WORKFLOW_BIN; no models or production stores"]
fn actual_batch_cli_success_failure_replay_and_fifo() {
    let root = std::env::temp_dir().join(format!(
        "batch-cli-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&root).unwrap();
    let script = root.join("exercise.py");
    std::fs::write(&script, include_str!("fixtures/enqueue_batch_cli.py")).unwrap();
    let output = process::capture(
        "/usr/bin/python3",
        &[
            script.to_string_lossy().into_owned(),
            std::env::var("LAB_CLI_BIN").unwrap(),
            std::env::var("LAB_WORKFLOW_BIN").unwrap(),
        ],
        &root,
        std::time::Duration::from_secs(180),
    )
    .unwrap();
    std::fs::remove_dir_all(&root).unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    println!("{}", String::from_utf8_lossy(&output.stdout));
}
