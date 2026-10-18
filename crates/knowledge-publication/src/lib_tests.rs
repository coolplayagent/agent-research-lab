use super::*;

fn report() -> OutcomeReport {
    OutcomeReport {
        title: "A bounded experiment".into(),
        summary: "A hypothesis pending evaluation.".into(),
        bindings: EvidenceBindings {
            experiment_id: "experiment-1".into(),
            source_repository: "https://github.com/coolplayagent/agent-research-lab".into(),
            source_commit: "a".repeat(40),
            prompt_sha256: "b".repeat(64),
            policy_sha256: "c".repeat(64),
            knowledge_revision: "d".repeat(40),
        },
        sources: vec![SourceReference {
            id: "reference".into(),
            title: "Primary reference".into(),
            resource: "https://example.org/paper".into(),
            sha256: Some("e".repeat(64)),
            retrieved_at: "2026-10-09T12:00:00+08:00".into(),
            access_status: "online-only".into(),
        }],
        findings: vec![Finding {
            kind: FindingKind::Inference,
            statement: "Recovery may help long tasks.".into(),
            source_ids: vec!["reference".into()],
            method: "Literature comparison".into(),
            limitations: "No experiment has tested this hypothesis.".into(),
            public_evidence_sha256: None,
        }],
        dissenting_views: vec!["Coordination overhead may outweigh the benefit.".into()],
        publication_reviewed: true,
    }
}

#[test]
fn citations_and_publication_boundary_are_validated() {
    let root = tempfile::tempdir().unwrap();
    let mut report = report();
    validate_report(&report, root.path()).unwrap();
    assert!(render_report(&report).contains("[^reference]"));
    report.findings[0].source_ids = vec!["missing".into()];
    assert!(validate_report(&report, root.path()).is_err());
    report.findings[0].source_ids = vec!["reference".into()];
    report.publication_reviewed = false;
    assert!(validate_report(&report, root.path()).is_err());
    report.publication_reviewed = true;
    report.summary = "private_holdout: secret".into();
    assert!(validate_report(&report, root.path()).is_err());
}

#[test]
fn source_archive_checksum_and_confinement_are_enforced() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("sources")).unwrap();
    fs::write(root.path().join("sources/paper.md"), "Public paper").unwrap();
    let mut report = report();
    report.sources[0].resource = "sources/paper.md".into();
    report.sources[0].sha256 = Some(file_sha256(&root.path().join("sources/paper.md")).unwrap());
    validate_report(&report, root.path()).unwrap();
    report.sources[0].resource = "sources/../private.md".into();
    assert!(validate_report(&report, root.path()).is_err());
}

#[cfg(unix)]
fn preparation_fixture() -> (tempfile::TempDir, PrepareReportRequest) {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("superpod");
    fs::create_dir(&root).unwrap();
    git(&root, &["init", "-b", "main"]).unwrap();
    git(&root, &["config", "user.name", "Test"]).unwrap();
    git(&root, &["config", "user.email", "test@example.org"]).unwrap();
    fs::create_dir_all(root.join("knowledge/software")).unwrap();
    fs::write(root.join("knowledge/software/index.md"), "# Software\n").unwrap();
    git(&root, &["add", "."]).unwrap();
    git(&root, &["commit", "-m", "baseline"]).unwrap();
    let binary = temp.path().join("relay-knowledge");
    fs::write(
        &binary,
        "#!/bin/sh\nprintf '{\"results\":[{\"valid\":true}],\"map\":{\"sources\":[]}}\\n'\n",
    )
    .unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
    let mut report = report();
    report.bindings.knowledge_revision = git(&root, &["rev-parse", "HEAD"]).unwrap();
    git(
        &root,
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/example/knowledge.git",
        ],
    )
    .unwrap();
    let request = PrepareReportRequest {
        layout: PublicationLayout {
            report_directory: "knowledge/software/ai-sdlc".into(),
            navigation_files: vec![
                "knowledge/software/index.md".into(),
                "knowledge/knowledge-map.yaml".into(),
            ],
            generated_roots: vec!["knowledge/topics".into()],
            topic: "ai-sdlc".into(),
            repository: "example/knowledge".into(),
            expected_origin: "https://github.com/example/knowledge.git".into(),
        },
        knowledge_root: root.clone(),
        worktree: temp.path().join("contribution"),
        relay_knowledge_binary: binary,
        report,
    };
    (temp, request)
}

#[cfg(unix)]
#[test]
fn preparation_isolated_from_main_checkout_and_retry_safe() {
    let (_temp, request) = preparation_fixture();
    let root = &request.knowledge_root;
    let prepared = prepare_report(&request).unwrap();
    assert!(!root.join(&prepared.report_path).exists());
    assert!(git(root, &["status", "--porcelain"]).unwrap().is_empty());
    assert_eq!(
        prepare_report(&request).unwrap().report_sha256,
        prepared.report_sha256
    );
}

#[cfg(unix)]
fn publication_fixture() -> (tempfile::TempDir, PublishReportRequest) {
    let (temp, request) = preparation_fixture();
    let prepared = prepare_report(&request).unwrap();
    (
        temp,
        PublishReportRequest {
            prepared,
            base_branch: "main".into(),
            title: "Publish test report".into(),
            body: "Test fixture".into(),
        },
    )
}

#[cfg(unix)]
#[test]
fn publication_rejects_extra_committed_files() {
    let (_temp, request) = publication_fixture();
    validate_prepared_publication(&request).unwrap();
    fs::write(
        request.prepared.worktree.join("unreviewed.txt"),
        "must not publish",
    )
    .unwrap();
    git(&request.prepared.worktree, &["add", "unreviewed.txt"]).unwrap();
    git(
        &request.prepared.worktree,
        &["commit", "-m", "unreviewed file"],
    )
    .unwrap();
    assert!(
        validate_prepared_publication(&request)
            .unwrap_err()
            .to_string()
            .contains("unexpected publication file")
    );
}

#[cfg(unix)]
#[test]
fn publication_rejects_forged_main_and_same_base_before_effects() {
    let (_temp, mut request) = publication_fixture();
    request.prepared.branch = "main".into();
    assert!(
        publish_report(&request)
            .unwrap_err()
            .to_string()
            .contains("dedicated outcome branch")
    );
    request.prepared.branch = format!("research/{}", request.prepared.experiment_id);
    request.base_branch = request.prepared.branch.clone();
    assert!(
        publish_report(&request)
            .unwrap_err()
            .to_string()
            .contains("dedicated outcome branch")
    );
    assert_eq!(
        git(&request.prepared.worktree, &["rev-parse", "HEAD"]).unwrap(),
        request.prepared.baseline
    );
}

#[cfg(unix)]
#[test]
fn publication_binds_index_map_and_committed_snapshot() {
    let (_temp, mut request) = publication_fixture();
    let index = request
        .prepared
        .worktree
        .join("knowledge/software/index.md");
    let original = fs::read(&index).unwrap();
    fs::write(&index, "changed after review").unwrap();
    assert!(validate_prepared_publication(&request).is_err());
    fs::write(&index, original).unwrap();
    let map = request
        .prepared
        .worktree
        .join("knowledge/knowledge-map.yaml");
    fs::write(&map, "fixture map").unwrap();
    request.prepared.publication_files = publication_snapshot(&request.prepared, None).unwrap();
    fs::write(&map, "map changed after review").unwrap();
    assert!(validate_prepared_publication(&request).is_err());
    fs::write(&map, "fixture map").unwrap();
    validate_prepared_publication(&request).unwrap();
    git(&request.prepared.worktree, &["add", "."]).unwrap();
    git(
        &request.prepared.worktree,
        &["commit", "-m", "reviewed contribution"],
    )
    .unwrap();
    let head = git(&request.prepared.worktree, &["rev-parse", "HEAD"]).unwrap();
    assert_eq!(
        publication_snapshot(&request.prepared, Some(&head)).unwrap(),
        request.prepared.publication_files
    );
    validate_prepared_publication(&request).unwrap();
}
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
