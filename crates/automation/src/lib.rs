//! Durable host-owned delivery outbox. Agent output is never an enqueue instruction.
//! The host must keep this directory and `evidence/` outside candidate writable sandboxes.

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", content = "request", rename_all = "snake_case")]
pub enum Operation {
    Install(delivery::InstallRequest),
    Pr(delivery::PullRequestRequest),
    Merge(delivery::MergeRequest),
    KnowledgePrepare(knowledge::PrepareReportRequest),
    KnowledgePublish(knowledge::PublishReportRequest),
    KnowledgeRefresh(knowledge::IndexRefreshRequest),
}

pub use contracts::QueueState;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QueueEntry {
    pub id: String,
    pub state: QueueState,
    pub attempts: u32,
    pub request_path: PathBuf,
    pub result: Option<Value>,
    pub error: Option<String>,
    pub updated_at: String,
    pub retry_after_unix: i64,
}

fn initialize(root: &Path) -> Result<storage::LockGuard> {
    for directory in [
        root.to_owned(),
        root.join("requests"),
        root.join("states"),
        root.join("evidence"),
    ] {
        if !directory.exists() {
            fs::create_dir_all(&directory)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
            }
        }
        ensure!(
            !fs::symlink_metadata(&directory)?.file_type().is_symlink(),
            "outbox directories cannot be symlinks"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            ensure!(
                fs::metadata(&directory)?.permissions().mode() & 0o077 == 0,
                "host outbox must not be accessible by other users"
            );
        }
    }
    storage::lock(&root.join("queue.lock"))
        .context("another host is processing the delivery outbox")
}

fn validate_provenance(root: &Path, operation: &Operation) -> Result<()> {
    let evidence = match operation {
        Operation::Install(request) => Some(&request.evidence),
        Operation::Merge(request) => Some(&request.evidence),
        _ => None,
    };
    if let Some(evidence) = evidence {
        let trusted = fs::canonicalize(root.join("evidence"))?;
        for artifact in [
            &evidence.qualitygate_report,
            &evidence.snapshot_receipt,
            &evidence.evaluation_receipt,
            evidence
                .skills_manifest
                .as_ref()
                .context("latest skills evidence is required")?,
        ] {
            ensure!(
                fs::canonicalize(&artifact.path)?.starts_with(&trusted),
                "delivery evidence must be host-owned under outbox/evidence"
            );
        }
        evidence.validate()?;
    }
    if let Operation::KnowledgePrepare(request) = operation {
        knowledge::validate_report(&request.report, &request.superpod_root)?;
    }
    Ok(())
}

fn save(root: &Path, entry: &mut QueueEntry) -> Result<()> {
    entry.updated_at = chrono::Utc::now().to_rfc3339();
    let path = root.join("states").join(format!("{}.json", entry.id));
    let temporary = path.with_extension("tmp");
    let mut file = File::create(&temporary)?;
    file.write_all(&serde_json::to_vec_pretty(entry)?)?;
    file.sync_all()?;
    fs::rename(temporary, path)?;
    File::open(root.join("states"))?.sync_all()?;
    Ok(())
}

fn load(root: &Path, id: &str) -> Result<QueueEntry> {
    delivery::validate_digest(id)?;
    let entry: QueueEntry =
        serde_json::from_slice(&fs::read(root.join("states").join(format!("{id}.json")))?)?;
    ensure!(entry.id == id, "outbox state identity mismatch");
    ensure!(
        entry.request_path
            == fs::canonicalize(root)?
                .join("requests")
                .join(format!("{id}.json")),
        "outbox request path escaped its host directory"
    );
    Ok(entry)
}

fn operation(entry: &QueueEntry) -> Result<Operation> {
    ensure!(
        fs::symlink_metadata(&entry.request_path)?
            .file_type()
            .is_file(),
        "outbox request must be a regular file"
    );
    let bytes = fs::read(&entry.request_path)?;
    ensure!(
        format!("{:x}", Sha256::digest(&bytes)) == entry.id,
        "immutable outbox request changed"
    );
    serde_json::from_slice(&bytes).context("invalid outbox operation")
}

/// Only a trusted host operator/controller may call this API after reviewing the operation.
/// Identical requests reuse their original state, including completed operations.
pub fn enqueue(outbox: &Path, request: &Operation) -> Result<QueueEntry> {
    let _lock = initialize(outbox)?;
    validate_provenance(outbox, request)?;
    let root = fs::canonicalize(outbox)?;
    let bytes = serde_json::to_vec(request)?;
    let id = format!("{:x}", Sha256::digest(&bytes));
    if root.join("states").join(format!("{id}.json")).exists() {
        let existing = load(&root, &id)?;
        operation(&existing)?;
        return Ok(existing);
    }
    let path = root.join("requests").join(format!("{id}.json"));
    if path.exists() {
        ensure!(fs::read(&path)? == bytes, "outbox request collision");
    } else {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o400))?;
        }
        File::open(root.join("requests"))?.sync_all()?;
    }
    let mut entry = QueueEntry {
        id,
        state: QueueState::Pending,
        attempts: 0,
        request_path: path,
        result: None,
        error: None,
        updated_at: String::new(),
        retry_after_unix: 0,
    };
    save(&root, &mut entry)?;
    Ok(entry)
}

fn execute(operation: &Operation) -> Result<Value> {
    match operation {
        Operation::Install(request) => {
            Ok(serde_json::to_value(delivery::install_candidate(request)?)?)
        }
        Operation::Pr(request) => Ok(serde_json::to_value(delivery::ensure_pull_request(
            request,
        )?)?),
        Operation::Merge(request) => Ok(serde_json::to_value(delivery::merge_pull_request(
            request,
        )?)?),
        Operation::KnowledgePrepare(request) => {
            Ok(serde_json::to_value(knowledge::prepare_report(request)?)?)
        }
        Operation::KnowledgePublish(request) => {
            Ok(serde_json::to_value(knowledge::publish_report(request)?)?)
        }
        Operation::KnowledgeRefresh(request) => {
            Ok(serde_json::to_value(knowledge::refresh_index(request)?)?)
        }
    }
}

/// Perform at most one authorized operation. An interrupted write is never retried blindly.
pub fn tick(outbox: &Path) -> Result<Option<QueueEntry>> {
    let _lock = initialize(outbox)?;
    let mut ids = Vec::new();
    for file in fs::read_dir(outbox.join("states"))? {
        let path = file?.path();
        if path.extension().is_some_and(|s| s == "json") {
            ids.push(
                path.file_stem()
                    .context("missing state ID")?
                    .to_string_lossy()
                    .into_owned(),
            );
        }
    }
    ids.sort();
    for id in ids {
        let mut entry = load(outbox, &id)?;
        if entry.state == QueueState::Running {
            entry.state = QueueState::NeedsReconciliation;
            entry.error = Some("Host stopped during an external operation; inspect actual remote/install/worktree state before explicit retry.".into());
            save(outbox, &mut entry)?;
            return Ok(Some(entry));
        }
        if !(entry.state == QueueState::Pending
            || (entry.state == QueueState::Waiting
                && entry.retry_after_unix <= chrono::Utc::now().timestamp()))
        {
            continue;
        }
        let operation = operation(&entry)?;
        if let Err(error) = validate_provenance(outbox, &operation) {
            entry.state = QueueState::NeedsReconciliation;
            entry.error = Some(format!(
                "Evidence validation failed before dispatch: {error:#}"
            ));
            save(outbox, &mut entry)?;
            return Ok(Some(entry));
        }
        entry.state = QueueState::Running;
        entry.attempts += 1;
        save(outbox, &mut entry)?;
        match execute(&operation) {
            Ok(result) => {
                entry.state = QueueState::Complete;
                entry.error = None;
                if matches!(operation, Operation::KnowledgeRefresh(_)) && result["ready"] == false {
                    entry.state = QueueState::Waiting;
                    entry.retry_after_unix = chrono::Utc::now().timestamp() + 30;
                }
                if matches!(operation, Operation::Merge(_))
                    && contracts::PullRequestState::from_value(&result["state"])
                        != Some(contracts::PullRequestState::Merged)
                {
                    entry.state = QueueState::NeedsReconciliation;
                    entry.error = Some("GitHub accepted the request but has not confirmed a merge; inspect the merge queue before retry.".into());
                }
                entry.result = Some(result);
            }
            Err(error) => {
                entry.state = QueueState::NeedsReconciliation;
                entry.error = Some(format!("{error:#}"));
            }
        }
        save(outbox, &mut entry)?;
        return Ok(Some(entry));
    }
    Ok(None)
}

/// Explicit host action after reconciling external effects. Adapters recheck current state.
pub fn reconcile_retry(outbox: &Path, id: &str) -> Result<QueueEntry> {
    let _lock = initialize(outbox)?;
    let mut entry = load(outbox, id)?;
    ensure!(
        entry.state == QueueState::NeedsReconciliation,
        "only reconciled blocked operations may retry"
    );
    let request = operation(&entry)?;
    validate_provenance(outbox, &request)?;
    entry.state = QueueState::Pending;
    entry.error = None;
    save(outbox, &mut entry)?;
    Ok(entry)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_enqueue_reuses_identity_and_failed_write_does_not_auto_retry() {
        let temp = tempfile::tempdir().unwrap();
        let outbox = temp.path().join("outbox");
        let request = Operation::KnowledgeRefresh(knowledge::IndexRefreshRequest {
            relay_knowledge_binary: temp.path().join("missing-cli"),
            superpod_root: temp.path().to_owned(),
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
            superpod_root: temp.path().to_owned(),
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
            let report = knowledge::ResearchReport {
                title: format!("Pending report {number}"),
                summary: "Hypothesis pending independent evaluation.".into(),
                bindings: knowledge::EvidenceBindings {
                    experiment_id: format!("pending-{number}"),
                    source_repository: "https://github.com/coolplayagent/agent-research-lab".into(),
                    source_commit: "a".repeat(40),
                    prompt_sha256: "b".repeat(64),
                    policy_sha256: "c".repeat(64),
                    superpod_commit: "d".repeat(40),
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
                superpod_root: superpod.clone(),
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
}
