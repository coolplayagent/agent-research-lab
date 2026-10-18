use super::*;

#[test]
fn default_launcher_reuses_its_inode_and_rejects_changed_bytes() {
    use std::os::unix::fs::MetadataExt;

    let directory = tempfile::tempdir().unwrap();
    let launcher = install_launcher(directory.path()).unwrap();
    let original = fs::metadata(&launcher).unwrap();
    assert_eq!(original.permissions().mode() & 0o777, 0o500);
    assert_eq!(install_launcher(directory.path()).unwrap(), launcher);
    let reused = fs::metadata(&launcher).unwrap();
    assert_eq!(
        (original.dev(), original.ino()),
        (reused.dev(), reused.ino())
    );

    fs::set_permissions(&launcher, fs::Permissions::from_mode(0o600)).unwrap();
    fs::write(&launcher, b"changed launcher").unwrap();
    assert!(install_launcher(directory.path()).is_err());
    assert_eq!(fs::read(&launcher).unwrap(), b"changed launcher");
}

#[test]
fn transport_success_and_partial_side_effects_are_not_business_success() {
    let good = json!({"provider":"bubblewrap","persistent_namespace":false,"status":"completed","exit_code":0,"timed_out":false,"network":"none"});
    require_execution(&good).unwrap();
    for (key, value) in [
        ("exit_code", json!(7)),
        ("status", json!("timeout")),
        ("timed_out", json!(true)),
        ("error", json!("lost result")),
        ("network", json!("host")),
    ] {
        let mut bad = good.clone();
        bad[key] = value;
        assert!(require_execution(&bad).is_err(), "{key}");
    }
    let ready = json!({"success":true,"provider":"bubblewrap","persistent_namespace":false,"status":"ready","network":"none","active_execution":null});
    require_session(&ready, contracts::SandboxSessionState::Ready).unwrap();
    assert!(require_session(&ready, contracts::SandboxSessionState::Stopped).is_err());
    let mut active = ready;
    active["active_execution"] = json!({"pid":12});
    assert!(require_session(&active, contracts::SandboxSessionState::Ready).is_err());
}
