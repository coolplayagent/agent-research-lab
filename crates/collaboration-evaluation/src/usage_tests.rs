use super::*;

#[test]
fn unreadable_or_unbounded_traces_never_become_usage_evidence() {
    let temp = tempfile::tempdir().unwrap();
    let trace = temp.path().join("stdout.jsonl");
    for bytes in [
        b"{truncated".as_slice(),
        b"{\"type\":\"turn.completed\",\"usage\":{\"input_tokens\":1}}\n".as_slice(),
        b"{\"type\":\"turn.completed\",\"usage\":{\"input_tokens\":18446744073709551615,\"output_tokens\":1}}\n{\"type\":\"turn.completed\",\"usage\":{\"input_tokens\":1,\"output_tokens\":1}}\n".as_slice(),
    ] {
        fs::write(&trace, bytes).unwrap();
        let observed = usage_trace(&trace);
        assert_eq!(observed["trust"], "worker_writable_diagnostic_only");
        assert_eq!(observed["observed"], Value::Null);
        assert!(observed["error"].as_str().is_some());
    }
    fs::File::create(&trace)
        .unwrap()
        .set_len(16 * 1024 * 1024 + 1)
        .unwrap();
    assert_eq!(usage_trace(&trace)["observed"], Value::Null);
    fs::remove_file(&trace).unwrap();
    let target = temp.path().join("elsewhere.jsonl");
    fs::write(
        &target,
        b"{\"type\":\"turn.completed\",\"usage\":{\"input_tokens\":9,\"output_tokens\":3}}\n",
    )
    .unwrap();
    std::os::unix::fs::symlink(&target, &trace).unwrap();
    assert_eq!(usage_trace(&trace)["observed"], Value::Null);
    assert_eq!(usage_trace(&target)["observed"]["input_tokens"], 9);
}
