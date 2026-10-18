//! Host admission and bounded queue recovery/dispatch under one store lock.
use crate::{
    dispatch,
    protocol::{Operation, QueueEntry, QueueState},
    provenance::validate_provenance,
    store::Outbox,
};
use anyhow::{Result, ensure};
use std::path::Path;

/// Only a trusted host operator/controller may call this API after reviewing the operation.
/// Identical requests reuse their original state, including completed operations.
pub fn enqueue(outbox: &Path, request: &Operation) -> Result<QueueEntry> {
    let store = Outbox::open(outbox)?;
    validate_provenance(store.root(), request)?;
    store.insert(request)
}

/// Perform at most one authorized operation. An interrupted write is never retried blindly.
pub fn tick(outbox: &Path) -> Result<Option<QueueEntry>> {
    run_one(outbox, dispatch::execute)
}

fn run_one(
    outbox: &Path,
    execute: impl FnOnce(&Operation) -> Result<dispatch::Outcome>,
) -> Result<Option<QueueEntry>> {
    let store = Outbox::open(outbox)?;
    for id in store.ids()? {
        let mut entry = store.load(&id)?;
        if entry.state == QueueState::Running {
            entry.state = QueueState::NeedsReconciliation;
            entry.error = Some("Host stopped during an external operation; inspect actual remote/install/worktree state before explicit retry.".into());
            store.save(&mut entry)?;
            return Ok(Some(entry));
        }
        if !(entry.state == QueueState::Pending
            || (entry.state == QueueState::Waiting
                && entry.retry_after_unix <= chrono::Utc::now().timestamp()))
        {
            continue;
        }
        let operation = match store.operation(&entry) {
            Ok(operation) => operation,
            Err(error) => {
                entry.state = QueueState::NeedsReconciliation;
                entry.error = Some(format!(
                    "Operation validation failed before dispatch: {error:#}"
                ));
                store.save(&mut entry)?;
                return Ok(Some(entry));
            }
        };
        if let Err(error) = validate_provenance(store.root(), &operation) {
            entry.state = QueueState::NeedsReconciliation;
            entry.error = Some(format!(
                "Evidence validation failed before dispatch: {error:#}"
            ));
            store.save(&mut entry)?;
            return Ok(Some(entry));
        }
        entry.state = QueueState::Running;
        entry.attempts += 1;
        store.save(&mut entry)?;
        match execute(&operation) {
            Ok(outcome) => {
                entry.state = outcome.state;
                entry.error = outcome.error;
                if entry.state == QueueState::Waiting {
                    entry.retry_after_unix = chrono::Utc::now().timestamp() + 30;
                }
                entry.result = Some(outcome.result);
            }
            Err(error) => {
                entry.state = QueueState::NeedsReconciliation;
                entry.error = Some(format!("{error:#}"));
            }
        }
        store.save(&mut entry)?;
        return Ok(Some(entry));
    }
    Ok(None)
}

/// Explicit host action after reconciling external effects. Adapters recheck current state.
pub fn reconcile_retry(outbox: &Path, id: &str) -> Result<QueueEntry> {
    let store = Outbox::open(outbox)?;
    let mut entry = store.load(id)?;
    ensure!(
        entry.state == QueueState::NeedsReconciliation,
        "only reconciled blocked operations may retry"
    );
    let request = store.operation(&entry)?;
    validate_provenance(store.root(), &request)?;
    entry.state = QueueState::Pending;
    entry.error = None;
    store.save(&mut entry)?;
    Ok(entry)
}

#[cfg(test)]
#[path = "queue_tests.rs"]
mod tests;
