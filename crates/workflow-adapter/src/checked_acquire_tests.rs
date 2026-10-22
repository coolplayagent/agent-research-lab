use super::*;
use std::os::unix::fs::PermissionsExt;

const TTL: u64 = 60000;
fn preconditions() -> AcquirePreconditionsV1 {
    AcquirePreconditionsV1 {
        expected_run_digest: format!("sha256:{}", "a".repeat(64)),
        expected_bundle_digest: format!("sha256:{}", "b".repeat(64)),
        expected_snapshot_digest: format!("sha256:{}", "c".repeat(64)),
        expected_execution_digest: format!("sha256:{}", "d".repeat(64)),
        expected_snapshot_revision: 1,
    }
}
fn lease() -> Value {
    let issued = now_ms().unwrap() - 1000;
    json!({"run_id":"run-1","owner":"owner-1","acquisition_id":"acquire-1",
        "epoch":1,"generation":"generation-1","issued_at_unix_ms":issued,
        "expires_at_unix_ms":issued + TTL})
}
struct Fixture {
    root: tempfile::TempDir,
    workflow: Workflow,
}
impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let binary = root.path().join("fixture-workflow");
        // This transport fixture records a real invocation and its exact request;
        // it does not represent a production transaction or execution result.
        fs::write(&binary, "#!/bin/sh\nset -eu\nprintf '%s\\n' invoked >> invocations\nprintf '%s\\n' \"$@\" > argv\ncat \"$4\" > received.json\ncat reply\ncat diagnostics >&2\nexit \"$(cat exit-code)\"\n").unwrap();
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
        // Some sandbox filesystems briefly report ETXTBSY after closing a new script.
        std::thread::sleep(std::time::Duration::from_millis(20));
        let workflow = Workflow::new(binary, root.path().join("private.db"), root.path());
        Self { root, workflow }
    }
    fn reply(&self, bytes: &[u8], status: u8, stderr: &[u8]) {
        fs::write(self.root.path().join("reply"), bytes).unwrap();
        fs::write(self.root.path().join("diagnostics"), stderr).unwrap();
        fs::write(self.root.path().join("exit-code"), status.to_string()).unwrap();
    }
    fn acquire(&self) -> Result<Value> {
        self.workflow
            .acquire_checked("run-1", "owner-1", "acquire-1", TTL, &preconditions())
    }
    fn count(&self) -> usize {
        fs::read_to_string(self.root.path().join("invocations"))
            .unwrap()
            .lines()
            .count()
    }
}
fn assert_failure(result: Result<Value>, expected: CheckedAcquireFailureCode) {
    let error = result.unwrap_err();
    assert_eq!(
        error.downcast_ref::<CheckedAcquireError>().unwrap().code,
        expected
    );
}

#[test]
fn exact_checked_request_and_complete_lease_cross_the_real_transport() {
    let fixture = Fixture::new();
    let lease = lease();
    fixture.reply(
        &serde_json::to_vec(&json!({"ok":true,"result":lease})).unwrap(),
        0,
        b"",
    );
    assert_eq!(fixture.acquire().unwrap(), lease);
    assert_eq!(fixture.count(), 1);
    let argv = fs::read_to_string(fixture.root.path().join("argv")).unwrap();
    let argv: Vec<_> = argv.lines().collect();
    assert_eq!(&argv[..2], &["run", "acquire-checked-v1"]);
    assert_eq!(argv[2], fixture.workflow.database.to_str().unwrap());
    assert_eq!(argv.len(), 4);
    let request: Value =
        serde_json::from_slice(&fs::read(fixture.root.path().join("received.json")).unwrap())
            .unwrap();
    let mut expected = serde_json::to_value(preconditions()).unwrap();
    expected["schema_version"] = json!(1);
    expected["lease"] =
        json!({"run_id":"run-1","owner":"owner-1","acquisition_id":"acquire-1","ttl_ms":TTL});
    assert_eq!(request, expected);
}

#[test]
fn only_the_exact_local_conflict_reply_proves_a_conflict() {
    let fixture = Fixture::new();
    let conflict = json!({"ok":false,"error":{"code":"admission_conflict",
        "message":"baseline differs","kernel_code":null}});
    fixture.reply(&serde_json::to_vec(&conflict).unwrap(), 1, b"");
    assert_failure(
        fixture.acquire(),
        CheckedAcquireFailureCode::AdmissionConflict,
    );
    let mut mixed = conflict.clone();
    mixed["result"] = lease();
    let mut uncertain = conflict.clone();
    uncertain["commit_may_have_succeeded"] = json!(true);
    let mut unknown = conflict.clone();
    unknown["error"]["code"] = json!("future_error");
    let mut kernel = conflict.clone();
    kernel["error"]["kernel_code"] = json!("lease_conflict");
    for (reply, status, stderr) in [
        (conflict.clone(), 0, b"".as_slice()),
        (conflict.clone(), 2, b"".as_slice()),
        (conflict, 1, b"unexpected".as_slice()),
        (mixed, 1, b"".as_slice()),
        (uncertain, 1, b"".as_slice()),
        (unknown, 1, b"".as_slice()),
        (kernel, 1, b"".as_slice()),
    ] {
        fixture.reply(&serde_json::to_vec(&reply).unwrap(), status, stderr);
        let before = fixture.count();
        assert_failure(fixture.acquire(), CheckedAcquireFailureCode::Unknown);
        assert_eq!(fixture.count(), before + 1);
    }
}

#[test]
fn incomplete_expired_or_changed_lease_authority_is_unknown() {
    let fixture = Fixture::new();
    let original = lease();
    for (field, value) in [
        ("run_id", json!("different")),
        ("owner", json!("different")),
        ("acquisition_id", json!("different")),
        ("epoch", json!(0)),
        ("generation", Value::Null),
        ("generation", json!("")),
        ("issued_at_unix_ms", json!(0)),
        ("issued_at_unix_ms", json!(now_ms().unwrap() + TTL)),
        ("expires_at_unix_ms", json!(now_ms().unwrap() - 1)),
        (
            "expires_at_unix_ms",
            json!(original["expires_at_unix_ms"].as_u64().unwrap() + 1),
        ),
    ] {
        let mut changed = original.clone();
        changed[field] = value;
        fixture.reply(
            &serde_json::to_vec(&json!({"ok":true,"result":changed})).unwrap(),
            0,
            b"",
        );
        assert_failure(fixture.acquire(), CheckedAcquireFailureCode::Unknown);
    }
    let mut missing = original.clone();
    missing.as_object_mut().unwrap().remove("epoch");
    let mut extra = original.clone();
    extra["future_authority"] = json!(true);
    for changed in [missing, extra] {
        fixture.reply(
            &serde_json::to_vec(&json!({"ok":true,"result":changed})).unwrap(),
            0,
            b"",
        );
        assert_failure(fixture.acquire(), CheckedAcquireFailureCode::Unknown);
    }
}

#[test]
fn duplicate_mixed_trailing_oversized_and_lost_replies_are_unknown_without_replay() {
    let fixture = Fixture::new();
    let valid = json!({"ok":true,"result":lease()});
    let mut mixed = valid.clone();
    mixed["error"] = json!({"code":"admission_conflict"});
    let nested_duplicate = serde_json::to_string(&valid)
        .unwrap()
        .replace("\"epoch\":1", "\"epoch\":1,\"epoch\":2");
    let replies = vec![
        b"{\"ok\":true,\"ok\":false,\"result\":{}}".to_vec(),
        nested_duplicate.into_bytes(),
        serde_json::to_vec(&mixed).unwrap(),
        format!("{} {{}}", valid).into_bytes(),
        b"{\"ok\":true}".to_vec(),
        vec![b' '; 8193],
        Vec::new(),
    ];
    for reply in replies {
        // A durable marker may exist even when the reply is lost. The adapter
        // cannot infer rollback, rebaseline or retry from that ambiguity.
        fs::write(
            fixture.root.path().join("possible-commit"),
            b"owned fixture marker",
        )
        .unwrap();
        fixture.reply(&reply, 0, b"");
        let before = fs::read_to_string(fixture.root.path().join("invocations"))
            .unwrap_or_default()
            .lines()
            .count();
        assert_failure(fixture.acquire(), CheckedAcquireFailureCode::Unknown);
        assert_eq!(fixture.count(), before + 1);
        assert!(fixture.root.path().join("possible-commit").exists());
    }
}
