use super::*;
use std::fs;

fn save(root: &Path, entry: &mut QueueEntry) -> Result<()> {
    Outbox::open(root)?.save(entry)
}
fn load(root: &Path, id: &str) -> Result<QueueEntry> {
    Outbox::open(root)?.load(id)
}

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
                access_status: knowledge::SourceAccessStatus::DownloadedVerified,
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

fn refresh_request(root: &Path, alias: &str) -> Operation {
    Operation::KnowledgeRefresh(knowledge::IndexRefreshRequest {
        relay_knowledge_binary: root.join("missing-cli"),
        knowledge_root: root.to_owned(),
        repository_alias: alias.into(),
        expected_commit: "a".repeat(40),
    })
}

fn outcome(state: QueueState) -> dispatch::Outcome {
    dispatch::Outcome {
        state,
        result: serde_json::json!({"fixture":true}),
        error: None,
    }
}

#[test]
fn running_intent_is_durable_and_lock_is_held_before_one_dispatch() {
    let temp = tempfile::tempdir().unwrap();
    let outbox = temp.path().join("outbox");
    let requests = [
        refresh_request(temp.path(), "first"),
        refresh_request(temp.path(), "second"),
    ];
    let mut entries = requests
        .iter()
        .map(|request| enqueue(&outbox, request).unwrap())
        .collect::<Vec<_>>();
    entries.sort_by(|a, b| a.id.cmp(&b.id));
    let completed = run_one(&outbox, |_| {
        let persisted: QueueEntry = serde_json::from_slice(
            &fs::read(
                outbox
                    .join("states")
                    .join(format!("{}.json", entries[0].id)),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(persisted.state, QueueState::Running);
        assert_eq!(persisted.attempts, 1);
        assert!(enqueue(&outbox, &requests[0]).is_err());
        assert!(tick(&outbox).is_err());
        assert!(reconcile_retry(&outbox, &entries[0].id).is_err());
        Ok(outcome(QueueState::Complete))
    })
    .unwrap()
    .unwrap();
    assert_eq!(completed.id, entries[0].id);
    assert_eq!(completed.state, QueueState::Complete);
    assert_eq!(load(&outbox, &entries[1].id).unwrap().attempts, 0);
    assert_eq!(
        load(&outbox, &entries[1].id).unwrap().state,
        QueueState::Pending
    );
    let original = entries
        .iter()
        .find(|entry| entry.id == completed.id)
        .unwrap();
    let request: Operation =
        serde_json::from_slice(&fs::read(&original.request_path).unwrap()).unwrap();
    assert_eq!(
        enqueue(&outbox, &request).unwrap().state,
        QueueState::Complete
    );
}

#[test]
fn waiting_refresh_obeys_backoff_and_resumes_only_when_due() {
    let temp = tempfile::tempdir().unwrap();
    let outbox = temp.path().join("outbox");
    let request = refresh_request(temp.path(), "knowledge");
    let original = enqueue(&outbox, &request).unwrap();
    let bytes = fs::read(&original.request_path).unwrap();
    let before = chrono::Utc::now().timestamp();
    let mut waiting = run_one(&outbox, |_| Ok(outcome(QueueState::Waiting)))
        .unwrap()
        .unwrap();
    let after = chrono::Utc::now().timestamp();
    assert_eq!(waiting.state, QueueState::Waiting);
    assert!((before + 30..=after + 30).contains(&waiting.retry_after_unix));
    assert!(
        run_one(&outbox, |_| panic!("waiting operation dispatched early"))
            .unwrap()
            .is_none()
    );
    assert_eq!(enqueue(&outbox, &request).unwrap().attempts, 1);
    waiting.retry_after_unix = 0;
    save(&outbox, &mut waiting).unwrap();
    let complete = run_one(&outbox, |_| Ok(outcome(QueueState::Complete)))
        .unwrap()
        .unwrap();
    assert_eq!(complete.state, QueueState::Complete);
    assert_eq!(complete.attempts, 2);
    assert_eq!(fs::read(&original.request_path).unwrap(), bytes);
    assert!(
        run_one(&outbox, |_| panic!("completed operation dispatched again"))
            .unwrap()
            .is_none()
    );
}

#[test]
fn changed_request_is_quarantined_without_dispatch_or_starving_later_work() {
    let temp = tempfile::tempdir().unwrap();
    let outbox = temp.path().join("outbox");
    let mut entries = ["first", "second"]
        .map(|alias| enqueue(&outbox, &refresh_request(temp.path(), alias)).unwrap());
    entries.sort_by(|a, b| a.id.cmp(&b.id));
    let corrupt = b"Changed request bytes";
    fs::remove_file(&entries[0].request_path).unwrap();
    fs::write(&entries[0].request_path, corrupt).unwrap();
    let blocked = run_one(&outbox, |_| panic!("changed request dispatched"))
        .unwrap()
        .unwrap();
    assert_eq!(blocked.id, entries[0].id);
    assert_eq!(blocked.state, QueueState::NeedsReconciliation);
    assert_eq!(blocked.attempts, 0);
    assert!(
        blocked
            .error
            .as_deref()
            .unwrap()
            .contains("immutable outbox request changed")
    );
    assert_eq!(fs::read(&blocked.request_path).unwrap(), corrupt);
    assert!(reconcile_retry(&outbox, &blocked.id).is_err());
    let next = run_one(&outbox, |_| Ok(outcome(QueueState::Complete)))
        .unwrap()
        .unwrap();
    assert_eq!(next.id, entries[1].id);
    assert_eq!(next.attempts, 1);
    assert!(
        run_one(&outbox, |_| panic!("blocked request retried"))
            .unwrap()
            .is_none()
    );
}

#[test]
fn unknown_persisted_operation_is_preserved_for_reconciliation() {
    use sha2::{Digest, Sha256};
    let temp = tempfile::tempdir().unwrap();
    let outbox = temp.path().join("outbox");
    let bytes = br#"{"kind":"future_operation","request":{}}"#;
    let id = format!("{:x}", Sha256::digest(bytes));
    {
        let store = Outbox::open(&outbox).unwrap();
        let request_path = store.root().join("requests").join(format!("{id}.json"));
        fs::write(&request_path, bytes).unwrap();
        store
            .save(&mut QueueEntry {
                id: id.clone(),
                state: QueueState::Pending,
                attempts: 0,
                request_path,
                result: None,
                error: None,
                updated_at: String::new(),
                retry_after_unix: 0,
            })
            .unwrap();
    }
    let blocked = run_one(&outbox, |_| panic!("unknown operation dispatched"))
        .unwrap()
        .unwrap();
    assert_eq!(blocked.state, QueueState::NeedsReconciliation);
    assert_eq!(blocked.attempts, 0);
    assert!(
        blocked
            .error
            .as_deref()
            .unwrap()
            .contains("invalid outbox operation")
    );
    assert_eq!(fs::read(&blocked.request_path).unwrap(), bytes);
    assert!(reconcile_retry(&outbox, &id).is_err());
    assert!(
        run_one(&outbox, |_| panic!("unknown operation retried"))
            .unwrap()
            .is_none()
    );
}
