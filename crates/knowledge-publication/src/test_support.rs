//! Shared local fixtures for publication boundary tests.
use crate::{
    EvidenceBindings, Finding, FindingKind, OutcomeReport, PrepareReportRequest, PublicationLayout,
    PublishReportRequest, SourceAccessStatus, SourceReference, host::git, prepare_report,
};
use std::fs;
pub(super) fn report() -> OutcomeReport {
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
            access_status: SourceAccessStatus::OnlineOnly,
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

#[cfg(unix)]
pub(super) fn preparation_fixture() -> (tempfile::TempDir, PrepareReportRequest) {
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
pub(super) fn publication_fixture() -> (tempfile::TempDir, PublishReportRequest) {
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
