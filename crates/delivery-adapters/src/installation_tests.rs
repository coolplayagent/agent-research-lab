use super::*;
use crate::tests::{artifact, evidence, mock_remote, upstream};

#[cfg(unix)]
fn install_fixture(request: &InstallRequest) -> Result<InstallReceipt> {
    install_candidate_with_client(
        request,
        &mock_remote(request.skill_source.parent().unwrap()),
    )
}

#[cfg(unix)]
fn fixture(failing: bool) -> (tempfile::TempDir, InstallRequest) {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("candidate");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("SKILL.md"), "---\nname: test-skill\n---\nTest").unwrap();
    fs::write(
        source.join("cli"),
        if failing {
            "#!/bin/sh\nexit 7\n"
        } else {
            "#!/bin/sh\nprintf 'version 1\\n'\n"
        },
    )
    .unwrap();
    fs::set_permissions(source.join("cli"), fs::Permissions::from_mode(0o755)).unwrap();
    let mut evidence = evidence(temp.path());
    let mut snapshot = evidence.snapshot_receipt.read().unwrap();
    snapshot["package_sha256"] = tree_sha256(&source).unwrap().into();
    snapshot["binary_sha256"] = file_sha256(&source.join("cli")).unwrap().into();
    evidence.snapshot_receipt = artifact(temp.path(), "snapshot.json", snapshot);
    let request = InstallRequest {
        skill_source: source.clone(),
        skills_root: temp.path().join("skills"),
        backup_root: temp.path().join("backups"),
        skill_directory_name: "test-skill".into(),
        binary_relative_path: "cli".into(),
        binary_sha256: file_sha256(&source.join("cli")).unwrap(),
        version: "1-dev".into(),
        evidence,
        smoke_arguments: vec!["--version".into()],
    };
    (temp, request)
}

#[cfg(unix)]
#[test]
fn installation_and_rollback_preserve_previous_version() {
    let (_temp, request) = fixture(false);
    let previous = request.skills_root.join("test-skill");
    fs::create_dir_all(&previous).unwrap();
    fs::write(previous.join("SKILL.md"), "---\nname: test-skill\n---\nOld").unwrap();
    let receipt = install_fixture(&request).unwrap();
    assert_eq!(receipt.phase.as_str(), "verified");
    assert_eq!(
        install_fixture(&request).unwrap().operation_id,
        receipt.operation_id
    );
    assert!(receipt.target.join("cli").exists());
    let rolled = rollback_install(&receipt.receipt_path).unwrap();
    assert_eq!(rolled.phase.as_str(), "rolled_back");
    assert!(
        fs::read_to_string(previous.join("SKILL.md"))
            .unwrap()
            .ends_with("Old")
    );
    assert_eq!(
        rollback_install(&receipt.receipt_path)
            .unwrap()
            .phase
            .as_str(),
        "rolled_back"
    );
}

#[cfg(unix)]
#[test]
fn failed_smoke_rolls_back_and_wrong_hash_never_activates() {
    let (_temp, mut request) = fixture(true);
    assert!(install_fixture(&request).is_err());
    assert!(!request.skills_root.join("test-skill").exists());
    request.binary_sha256 = "0".repeat(64);
    assert!(install_fixture(&request).is_err());
    assert!(!request.skills_root.join("test-skill").exists());
}

#[cfg(unix)]
#[test]
fn interrupted_backup_is_recovered_before_reinstall() {
    let (_temp, request) = fixture(false);
    let previous = request.skills_root.join("test-skill");
    fs::create_dir_all(&previous).unwrap();
    fs::write(previous.join("SKILL.md"), "---\nname: test-skill\n---\nOld").unwrap();
    let mut interrupted = install_fixture(&request).unwrap();
    let staged = interrupted.receipt_path.parent().unwrap().join("candidate");
    fs::rename(&interrupted.target, staged).unwrap();
    interrupted.phase = InstallationPhase::PreviousBackedUp;
    write_receipt(&interrupted).unwrap();
    let replacement = install_fixture(&request).unwrap();
    assert_eq!(replacement.phase.as_str(), "verified");
    assert_eq!(
        fs::read_to_string(replacement.backup.unwrap().join("SKILL.md")).unwrap(),
        "---\nname: test-skill\n---\nOld"
    );
}

#[cfg(unix)]
#[test]
fn rollback_refuses_to_overwrite_a_later_modified_installation() {
    let (_temp, request) = fixture(false);
    let receipt = install_fixture(&request).unwrap();
    fs::write(receipt.target.join("newer-file"), "later change").unwrap();
    assert!(rollback_install(&receipt.receipt_path).is_err());
    assert!(receipt.target.join("newer-file").exists());
}

#[cfg(unix)]
#[test]
fn installation_rejects_advanced_or_unverifiable_upstream_before_activation() {
    let (temp, request) = fixture(false);
    let client = mock_remote(temp.path());
    fs::write(
        temp.path().join("current-upstream.json"),
        serde_json::to_vec(&json!({"sha":"1".repeat(40)})).unwrap(),
    )
    .unwrap();
    assert!(install_candidate_with_client(&request, &client).is_err());
    assert!(!request.skills_root.join("test-skill").exists());
    fs::write(temp.path().join("current-upstream.json"), "{}").unwrap();
    assert!(install_candidate_with_client(&request, &client).is_err());
    assert!(!request.skills_root.join("test-skill").exists());
    fs::write(
        temp.path().join("current-upstream.json"),
        serde_json::to_vec(&json!({"sha":upstream().commit})).unwrap(),
    )
    .unwrap();
    fs::write(
        temp.path().join("current-release.json"),
        r#"{"id":2,"tag_name":"v2","draft":false,"prerelease":false}"#,
    )
    .unwrap();
    assert!(install_candidate_with_client(&request, &client).is_err());
    assert!(!request.skills_root.join("test-skill").exists());
    fs::write(
        &request.evidence.skills_manifest.as_ref().unwrap().path,
        "{}",
    )
    .unwrap();
    assert!(install_candidate_with_client(&request, &client).is_err());
    assert!(!request.skills_root.join("test-skill").exists());
}

#[cfg(unix)]
#[test]
fn unknown_journal_phase_blocks_recovery_without_changing_installed_files() {
    let (_temp, request) = fixture(false);
    let target = request.skills_root.join("test-skill");
    fs::create_dir_all(&target).unwrap();
    fs::write(target.join("SKILL.md"), "---\nname: test-skill\n---\nOld").unwrap();
    let receipt = install_fixture(&request).unwrap();
    let target_hash = tree_sha256(&target).unwrap();
    let backup = receipt.backup.as_ref().unwrap();
    let backup_hash = tree_sha256(backup).unwrap();
    let original = fs::read(&receipt.receipt_path).unwrap();
    for phase in [
        json!("future_phase"),
        json!("active"),
        Value::Null,
        json!(1),
    ] {
        let mut corrupt: Value = serde_json::from_slice(&original).unwrap();
        corrupt["phase"] = phase;
        let bytes = serde_json::to_vec(&corrupt).unwrap();
        fs::write(&receipt.receipt_path, &bytes).unwrap();
        assert!(rollback_install(&receipt.receipt_path).is_err());
        assert!(install_fixture(&request).is_err());
        assert_eq!(fs::read(&receipt.receipt_path).unwrap(), bytes);
        assert_eq!(tree_sha256(&target).unwrap(), target_hash);
        assert_eq!(tree_sha256(backup).unwrap(), backup_hash);
    }
    fs::write(&receipt.receipt_path, original).unwrap();
    assert_eq!(
        rollback_install(&receipt.receipt_path).unwrap().phase,
        InstallationPhase::RolledBack
    );
}

#[cfg(unix)]
#[test]
fn restored_files_settle_an_interrupted_rollback_without_requiring_the_moved_backup() {
    let (_temp, request) = fixture(false);
    let target = request.skills_root.join("test-skill");
    fs::create_dir_all(&target).unwrap();
    fs::write(target.join("SKILL.md"), "---\nname: test-skill\n---\nOld").unwrap();
    let old_hash = tree_sha256(&target).unwrap();
    let mut receipt = install_fixture(&request).unwrap();
    let rejected = receipt.receipt_path.parent().unwrap().join("rejected");
    fs::rename(&target, &rejected).unwrap();
    fs::rename(receipt.backup.as_ref().unwrap(), &target).unwrap();
    receipt.phase = InstallationPhase::Activated;
    write_receipt(&receipt).unwrap();
    let settled = rollback_install(&receipt.receipt_path).unwrap();
    assert_eq!(settled.phase, InstallationPhase::RolledBack);
    assert_eq!(tree_sha256(&target).unwrap(), old_hash);
    assert_eq!(tree_sha256(&rejected).unwrap(), receipt.package_sha256);
    assert_eq!(
        rollback_install(&receipt.receipt_path).unwrap().phase,
        settled.phase
    );
}
