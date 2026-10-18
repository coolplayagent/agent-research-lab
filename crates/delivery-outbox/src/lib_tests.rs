use super::*;

#[test]
fn duplicate_enqueue_reuses_identity_and_failed_write_does_not_auto_retry() {
    let temp = tempfile::tempdir().unwrap();
    let outbox = temp.path().join("outbox");
    let request = Operation::KnowledgeRefresh(knowledge::IndexRefreshRequest {
        relay_knowledge_binary: temp.path().join("missing-cli"),
        knowledge_root: temp.path().to_owned(),
        repository_alias: "superpod".into(),
        expected_commit: "a".repeat(40),
    });
    let first = enqueue(&outbox, &request).unwrap();
    assert_eq!(first.id, enqueue(&outbox, &request).unwrap().id);
    let failed = tick(&outbox).unwrap().unwrap();
    assert_eq!(failed.state, QueueState::NeedsReconciliation);
    assert!(tick(&outbox).unwrap().is_none());
    assert_eq!(enqueue(&outbox, &request).unwrap().attempts, 1);
    reconcile_retry(&outbox, &first.id).unwrap();
    assert_eq!(tick(&outbox).unwrap().unwrap().attempts, 2);
}

#[test]
fn interrupted_operation_requires_reconciliation() {
    let temp = tempfile::tempdir().unwrap();
    let outbox = temp.path().join("outbox");
    let request = Operation::KnowledgeRefresh(knowledge::IndexRefreshRequest {
        relay_knowledge_binary: temp.path().join("missing-cli"),
        knowledge_root: temp.path().to_owned(),
        repository_alias: "superpod".into(),
        expected_commit: "a".repeat(40),
    });
    let mut entry = enqueue(&outbox, &request).unwrap();
    entry.state = QueueState::Running;
    save(&outbox, &mut entry).unwrap();
    assert_eq!(
        tick(&outbox).unwrap().unwrap().state,
        QueueState::NeedsReconciliation
    );
}
#[test]
fn changed_pending_provenance_is_quarantined_without_starving_next_entry() {
    let temp = tempfile::tempdir().unwrap();
    let outbox = temp.path().join("outbox");
    let superpod = temp.path().join("superpod");
    fs::create_dir_all(superpod.join("sources")).unwrap();
    let mut entries = Vec::new();
    for number in 1..=2 {
        let relative = format!("sources/reference-{number}.md");
        let source = superpod.join(&relative);
        fs::write(&source, format!("Public fixture source {number}")).unwrap();
        let report = knowledge::OutcomeReport {
            title: format!("Pending report {number}"),
            summary: "Hypothesis pending independent evaluation.".into(),
            bindings: knowledge::EvidenceBindings {
                experiment_id: format!("pending-{number}"),
                source_repository: "https://github.com/coolplayagent/agent-research-lab".into(),
                source_commit: "a".repeat(40),
                prompt_sha256: "b".repeat(64),
                policy_sha256: "c".repeat(64),
                knowledge_revision: "d".repeat(40),
            },
            sources: vec![knowledge::SourceReference {
                id: "reference".into(),
                title: "Public fixture".into(),
                resource: relative,
                sha256: Some(delivery::file_sha256(&source).unwrap()),
                retrieved_at: "2026-10-09T12:00:00+08:00".into(),
                access_status: "downloaded-verified".into(),
            }],
            findings: vec![knowledge::Finding {
                kind: knowledge::FindingKind::Inference,
                statement: "Recovery may improve continuity.".into(),
                source_ids: vec!["reference".into()],
                method: "Fixture source comparison".into(),
                limitations: "No empirical validation is claimed.".into(),
                public_evidence_sha256: None,
            }],
            dissenting_views: vec![],
            publication_reviewed: true,
        };
        let request = Operation::KnowledgePrepare(knowledge::PrepareReportRequest {
            layout: knowledge::PublicationLayout {
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
            knowledge_root: superpod.clone(),
            worktree: temp.path().join(format!("worktree-{number}")),
            relay_knowledge_binary: temp.path().join("missing-cli"),
            report,
        });
        entries.push((enqueue(&outbox, &request).unwrap(), source));
    }
    entries.sort_by(|a, b| a.0.id.cmp(&b.0.id));
    // Source bytes change after admission, while the immutable operation stays intact.
    fs::write(&entries[0].1, "Source changed after publication review").unwrap();
    let quarantined = tick(&outbox).unwrap().unwrap();
    assert_eq!(quarantined.id, entries[0].0.id);
    assert_eq!(quarantined.state, QueueState::NeedsReconciliation);
    assert_eq!(
        quarantined.attempts, 0,
        "invalid evidence must never dispatch"
    );
    assert!(
        quarantined
            .error
            .as_deref()
            .unwrap()
            .contains("Evidence validation failed before dispatch")
    );
    assert!(
        quarantined
            .error
            .as_deref()
            .unwrap()
            .contains("checksum mismatch")
    );
    assert_eq!(
        load(&outbox, &quarantined.id).unwrap().state,
        QueueState::NeedsReconciliation
    );
    let next = tick(&outbox).unwrap().unwrap();
    assert_eq!(
        next.id, entries[1].0.id,
        "quarantined entry cannot starve later work"
    );
    assert_eq!(next.attempts, 1, "next valid request must reach dispatch");
    // This fixture intentionally has no Git checkout/CLI, so dispatch cannot publish.
    assert_eq!(next.state, QueueState::NeedsReconciliation);
    assert!(
        tick(&outbox).unwrap().is_none(),
        "neither blocked entry retries automatically"
    );
}
