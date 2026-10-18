use super::*;

fn upstream() -> RemoteSnapshot {
    RemoteSnapshot {
        repository: "coolplayagent/example".into(),
        default_branch: "main".into(),
        commit: "0".repeat(40),
        checked_at: "2026-10-09T00:00:00Z".into(),
    }
}

#[cfg(unix)]
fn mock_remote(root: &Path) -> RemoteClient {
    use std::os::unix::fs::PermissionsExt;
    let head = root.join("current-upstream.json");
    fs::write(
        &head,
        serde_json::to_vec(&json!({"sha":upstream().commit})).unwrap(),
    )
    .unwrap();
    let gh = root.join("fake-gh");
    let quoted = head.to_string_lossy().replace('\'', "'\\''");
    let release = root.join("current-release.json");
    if !release.exists() {
        fs::write(
            &release,
            r#"{"id":1,"tag_name":"v1","draft":false,"prerelease":false}"#,
        )
        .unwrap();
    }
    let release = release.to_string_lossy().replace('\'', "'\\''");
    fs::write(&gh, format!("#!/bin/sh\nset -eu\ncase \"$1\" in\nrepo) printf '%s\\n' '{{\"nameWithOwner\":\"coolplayagent/example\",\"defaultBranchRef\":{{\"name\":\"main\"}}}}';;\napi) case \"$2\" in */releases/latest) cat '{release}';; *) cat '{quoted}';; esac;;\n*) exit 92;;\nesac\n")).unwrap();
    fs::set_permissions(&gh, fs::Permissions::from_mode(0o755)).unwrap();
    RemoteClient {
        git: PathBuf::from("git"),
        gh,
        timeout: Duration::from_secs(2),
    }
}

#[cfg(unix)]
fn install_fixture(request: &InstallRequest) -> Result<InstallReceipt> {
    install_candidate_with_client(
        request,
        &mock_remote(request.skill_source.parent().unwrap()),
    )
}

fn artifact(root: &Path, name: &str, value: Value) -> VerificationArtifact {
    let path = root.join(name);
    fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    VerificationArtifact {
        sha256: file_sha256(&path).unwrap(),
        path,
    }
}

fn evidence(root: &Path) -> ReleaseEvidence {
    let installed = root.join("verified-skill");
    fs::create_dir_all(&installed).unwrap();
    fs::write(
        installed.join("SKILL.md"),
        "---\nname: verified-skill\n---\nFixture",
    )
    .unwrap();
    fs::write(installed.join("cli"), "verified runtime").unwrap();
    let skills_manifest = artifact(
        root,
        "skills-manifest.json",
        json!({"schema_version":1,"skills":[{"name":"verified-skill","repository":"coolplayagent/example","installed_path":installed,"runtime_path":installed.join("cli"),"tree_sha256":freshness::skill_tree_digest(&installed).unwrap(),"runtime_sha256":file_sha256(&installed.join("cli")).unwrap(),"release_tag":"v1","release_id":1,"source_commit":null,"checked_at":"2026-10-09T00:00:00Z"}]}),
    );
    let qualitygate_report = artifact(
        root,
        "qualitygate.json",
        json!({ "profile": "full", "scope": "delivery", "gate": {"complete":true,"decision":"pass"}, "plan":{"pending_delivery_checks":[]},"snapshot":{"content_digest":"e".repeat(64),"verification_digest":"f".repeat(64)} }),
    );
    let snapshot_receipt = artifact(
        root,
        "snapshot.json",
        json!({"source_commit":"a".repeat(40),"upstream":upstream(),"skills_manifest_sha256":skills_manifest.sha256,"prompt_sha256":"b".repeat(64),"policy_sha256":"c".repeat(64),"superpod_commit":"d".repeat(40),"qualitygate_report_sha256":qualitygate_report.sha256,"content_digest":"e".repeat(64),"verification_digest":"f".repeat(64)}),
    );
    let evaluation_receipt = artifact(
        root,
        "evaluation.json",
        json!({"role":"independent_evaluator","status":"pass","complete":true,"source_commit":"a".repeat(40),"upstream":upstream(),"skills_manifest_sha256":skills_manifest.sha256,"prompt_sha256":"b".repeat(64),"policy_sha256":"c".repeat(64),"superpod_commit":"d".repeat(40),"evaluator_id":"evaluation","implementer_id":"implementation"}),
    );
    ReleaseEvidence {
        source_commit: "a".repeat(40),
        upstream: Some(upstream()),
        skills_manifest: Some(skills_manifest),
        prompt_sha256: "b".repeat(64),
        policy_sha256: "c".repeat(64),
        superpod_commit: "d".repeat(40),
        implementer_id: "implementation".into(),
        evaluator_id: "evaluation".into(),
        evaluation_passed: true,
        qualitygate_passed: true,
        qualitygate_report,
        snapshot_receipt,
        evaluation_receipt,
    }
}

#[test]
fn merge_rejects_stale_unknown_failed_and_unresolved_evidence() {
    let temp = tempfile::tempdir().unwrap();
    let request = MergeRequest {
        repository: "coolplayagent/example".into(),
        working_directory: PathBuf::from("."),
        number: 1,
        expected_head: "a".repeat(40),
        evidence: evidence(temp.path()),
    };
    let valid = MergeSnapshot {
        head: request.expected_head.clone(),
        base_ref: "main".into(),
        base_commit: upstream().commit.clone(),
        default_branch: "main".into(),
        default_commit: upstream().commit,
        state: "OPEN".into(),
        draft: false,
        author: "author".into(),
        mergeable: "MERGEABLE".into(),
        merge_state: "CLEAN".into(),
        review_decision: Some("APPROVED".into()),
        checks: vec!["SUCCESS".into()],
        reviews: vec![ReviewRecord {
            author: "reviewer".into(),
            state: "APPROVED".into(),
            commit: request.expected_head.clone(),
        }],
        unresolved_threads: 0,
        complete: true,
    };
    evaluate_merge_gate(&request, &valid).unwrap();
    let mut stale = valid.clone();
    stale.head = "e".repeat(40);
    assert!(evaluate_merge_gate(&request, &stale).is_err());
    let mut advanced = valid.clone();
    advanced.base_commit = "1".repeat(40);
    advanced.default_commit = advanced.base_commit.clone();
    assert!(evaluate_merge_gate(&request, &advanced).is_err());
    let mut retargeted = valid.clone();
    retargeted.base_ref = "old-release".into();
    assert!(evaluate_merge_gate(&request, &retargeted).is_err());
    let mut incomplete = valid.clone();
    incomplete.complete = false;
    assert!(evaluate_merge_gate(&request, &incomplete).is_err());
    let mut pending = valid.clone();
    pending.checks = vec!["PENDING".into()];
    assert!(evaluate_merge_gate(&request, &pending).is_err());
    let mut unresolved = valid.clone();
    unresolved.unresolved_threads = 1;
    assert!(evaluate_merge_gate(&request, &unresolved).is_err());
    let mut missing_review = valid.clone();
    missing_review.review_decision = Some("REVIEW_REQUIRED".into());
    assert!(evaluate_merge_gate(&request, &missing_review).is_err());
    let mut no_required_review = valid;
    no_required_review.reviews.clear();
    no_required_review.review_decision = None;
    evaluate_merge_gate(&request, &no_required_review).unwrap();
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
    assert_eq!(receipt.phase, "verified");
    assert_eq!(
        install_fixture(&request).unwrap().operation_id,
        receipt.operation_id
    );
    assert!(receipt.target.join("cli").exists());
    let rolled = rollback_install(&receipt.receipt_path).unwrap();
    assert_eq!(rolled.phase, "rolled_back");
    assert!(
        fs::read_to_string(previous.join("SKILL.md"))
            .unwrap()
            .ends_with("Old")
    );
    assert_eq!(
        rollback_install(&receipt.receipt_path).unwrap().phase,
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
    interrupted.phase = "previous_backed_up".into();
    write_receipt(&interrupted).unwrap();
    let replacement = install_fixture(&request).unwrap();
    assert_eq!(replacement.phase, "verified");
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

#[test]
fn booleans_do_not_override_tampered_or_stale_artifacts() {
    let temp = tempfile::tempdir().unwrap();
    let mut evidence = evidence(temp.path());
    evidence.validate().unwrap();
    fs::write(&evidence.qualitygate_report.path, "{}").unwrap();
    assert!(evidence.validate().is_err());
    evidence.qualitygate_report.sha256 = file_sha256(&evidence.qualitygate_report.path).unwrap();
    assert!(evidence.validate().is_err());
}

#[test]
fn historical_evidence_can_be_read_but_cannot_pass_new_release_gates() {
    let temp = tempfile::tempdir().unwrap();
    let mut old = serde_json::to_value(evidence(temp.path())).unwrap();
    old.as_object_mut().unwrap().remove("upstream");
    let old: ReleaseEvidence = serde_json::from_value(old).unwrap();
    assert!(old.upstream.is_none());
    assert!(old.validate().is_err());
    let mut without_skills = serde_json::to_value(evidence(temp.path())).unwrap();
    without_skills
        .as_object_mut()
        .unwrap()
        .remove("skills_manifest");
    let without_skills: ReleaseEvidence = serde_json::from_value(without_skills).unwrap();
    assert!(without_skills.validate().is_err());
    let mut mismatched = evidence(temp.path());
    mismatched.upstream.as_mut().unwrap().commit = "1".repeat(40);
    assert!(mismatched.validate().is_err());
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
