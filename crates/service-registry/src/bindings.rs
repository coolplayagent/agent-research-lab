//! Durable, idempotent validation operations. A restart never implies success.
use anyhow::{Context, Result, ensure};
use contracts::{
    BindingErrorCode as Code, BindingOperationState as State, BindingStage,
    BindingStepState as Step,
};
use serde::{Deserialize, Serialize};
use service_api::*;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Mutex,
};
use tokio::sync::watch;

pub(super) struct Operations {
    path: PathBuf,
    entries: Mutex<Entries>,
}
#[derive(Default)]
struct Entries {
    active: BTreeMap<String, Entry>,
    retired: BTreeMap<String, Retired>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Retired {
    fingerprint: String,
    started_unix_ms: u64,
}
#[derive(Serialize, Deserialize)]
struct Journal {
    operations: Vec<BindingOperation>,
    retired: BTreeMap<String, Retired>,
}
#[derive(Deserialize)]
#[serde(untagged)]
enum StoredJournal {
    Current(Journal),
    Legacy(Vec<BindingOperation>),
}
struct Entry {
    record: BindingOperation,
    cancel: watch::Sender<bool>,
}
pub(super) fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
pub(super) fn diagnostic(
    id: &str,
    stage: BindingStage,
    code: Code,
    error: Option<&anyhow::Error>,
) -> BindingDiagnostic {
    BindingDiagnostic {
        code,
        stage,
        diagnostic_id: format!("binding-{id}"),
        os_code: error.and_then(service_protocol::os_code),
        operation: stage.to_string(),
    }
}
impl Operations {
    pub fn open(state: &Path, config: &Configuration) -> Result<Self> {
        let path = state.join("private/binding-operations.json");
        let (mut records, retired) = if path.exists() {
            match storage::read(&path)? {
                StoredJournal::Current(journal) => (journal.operations, journal.retired),
                StoredJournal::Legacy(records) => (records, BTreeMap::new()),
            }
        } else {
            (vec![], BTreeMap::new())
        };
        ensure!(
            records.len() <= 256 && retired.len() <= 256,
            "binding journal exceeds bound"
        );
        // Legacy journals have no global sequence; use the request ID as a stable tie-break.
        records.sort_by(|a, b| {
            (a.started_unix_ms, &a.request.request_id)
                .cmp(&(b.started_unix_ms, &b.request.request_id))
        });
        let mut entries = Entries {
            active: BTreeMap::new(),
            retired,
        };
        for mut record in records {
            record.request.validate()?;
            if record.state == State::Running {
                if let Some(evidence) = config
                    .binding_evidence
                    .get(&record.request.person_id)
                    .filter(|e| {
                        e.request_id == record.request.request_id
                            && config.bindings.get(&record.request.person_id)
                                == Some(&record.request.adapter_id)
                            && config
                                .adapters
                                .iter()
                                .find(|a| a.id == record.request.adapter_id)
                                .is_some_and(|adapter| {
                                    serde_json::to_vec(adapter).is_ok_and(|bytes| {
                                        storage::digest(&bytes) == e.adapter_digest
                                    })
                                })
                    })
                {
                    record.state = State::Succeeded;
                    record.configuration_revision = Some(config.revision);
                    record.evidence = Some(evidence.clone());
                    for step in &mut record.steps {
                        step.state = Step::Passed;
                    }
                } else {
                    record.state = State::Interrupted;
                    let stage = record
                        .steps
                        .iter()
                        .find(|s| s.state == Step::Running)
                        .map_or(BindingStage::Discover, |s| s.stage);
                    record.diagnostic = Some(diagnostic(
                        &record.request.request_id,
                        stage,
                        Code::Interrupted,
                        None,
                    ));
                    for step in &mut record.steps {
                        if step.state == Step::Running {
                            step.state = Step::Failed;
                        }
                    }
                }
                record.sequence += 1;
            }
            if let Some(old) = entries
                .active
                .values()
                .find(|e| e.record.request.person_id == record.request.person_id)
                .map(|e| e.record.request.request_id.clone())
            {
                retire(&mut entries, &old)?;
            }
            entries.active.insert(
                record.request.request_id.clone(),
                Entry {
                    record,
                    cancel: watch::channel(false).0,
                },
            );
        }
        let operations = Self {
            path,
            entries: Mutex::new(entries),
        };
        let entries = operations.entries.lock().unwrap();
        operations.persist(&entries)?;
        operations.cleanup(&entries)?;
        drop(entries);
        Ok(operations)
    }
    fn persist(&self, entries: &Entries) -> Result<()> {
        storage::write(
            &self.path,
            &Journal {
                operations: entries.active.values().map(|e| e.record.clone()).collect(),
                retired: entries.retired.clone(),
            },
        )
    }
    fn cleanup(&self, entries: &Entries) -> Result<()> {
        for file in std::fs::read_dir(self.path.parent().context("binding directory missing")?)? {
            let file = file?;
            let name = file.file_name();
            let name = name.to_string_lossy();
            if let Some(id) = name
                .strip_prefix("binding-")
                .and_then(|n| n.strip_suffix(".diagnostic.json"))
                && !entries.active.contains_key(id)
            {
                std::fs::remove_file(file.path())?;
            }
        }
        Ok(())
    }
    pub fn list(&self) -> Vec<BindingOperation> {
        self.entries
            .lock()
            .unwrap()
            .active
            .values()
            .map(|e| e.record.clone())
            .collect()
    }
    pub fn get(&self, id: &str) -> Result<BindingOperation> {
        let entries = self.entries.lock().unwrap();
        ensure!(
            !entries.retired.contains_key(id),
            crate::binding_flow::Failure(Code::Superseded)
        );
        entries
            .active
            .get(id)
            .map(|e| e.record.clone())
            .context("binding operation not found")
    }
    pub fn replay(&self, request: &BindingRequest) -> Result<Option<BindingOperation>> {
        replay(&self.entries.lock().unwrap(), request)
    }
    pub fn begin(
        &self,
        request: BindingRequest,
    ) -> Result<(BindingOperation, Option<watch::Receiver<bool>>)> {
        let mut entries = self.entries.lock().unwrap();
        if let Some(record) = replay(&entries, &request)? {
            return Ok((record, None));
        }
        let running = entries
            .active
            .values()
            .filter(|e| e.record.state == State::Running)
            .collect::<Vec<_>>();
        ensure!(
            running.len() < 4
                && !running
                    .iter()
                    .any(|e| e.record.request.person_id == request.person_id),
            crate::binding_flow::Failure(Code::Busy)
        );
        let old = entries
            .active
            .values()
            .find(|e| e.record.request.person_id == request.person_id)
            .or_else(|| {
                if entries.active.len() >= 256 {
                    entries
                        .active
                        .values()
                        .filter(|e| e.record.state != State::Running)
                        .min_by_key(|e| e.record.started_unix_ms)
                } else {
                    None
                }
            })
            .map(|e| e.record.request.request_id.clone());
        let retired_before = entries.retired.clone();
        let replaced = if let Some(old) = &old {
            Some(retire(&mut entries, old)?)
        } else {
            None
        };
        let record = BindingOperation {
            request,
            state: State::Running,
            sequence: 0,
            started_unix_ms: now(),
            steps: BindingStage::ALL
                .iter()
                .map(|stage| BindingStep {
                    stage: *stage,
                    state: Step::Pending,
                    elapsed_ms: 0,
                })
                .collect(),
            program: None,
            diagnostic: None,
            configuration_revision: None,
            evidence: None,
        };
        let (cancel, receiver) = watch::channel(false);
        let id = record.request.request_id.clone();
        entries.active.insert(
            id.clone(),
            Entry {
                record: record.clone(),
                cancel,
            },
        );
        if let Err(error) = self.persist(&entries) {
            entries.active.remove(&id);
            entries.retired = retired_before;
            if let (Some(old), Some(replaced)) = (old, replaced) {
                entries.active.insert(old, replaced);
            }
            return Err(error.context(crate::binding_flow::Failure(Code::PersistenceFailed)));
        }
        if let Err(error) = self.cleanup(&entries) {
            let entry = entries.active.get_mut(&id).unwrap();
            entry.record.state = State::Failed;
            entry.record.sequence += 1;
            entry.record.diagnostic = Some(diagnostic(
                &id,
                BindingStage::Discover,
                Code::PersistenceFailed,
                Some(&error),
            ));
            for step in &mut entry.record.steps {
                step.state = Step::Skipped;
            }
            self.persist(&entries)?;
            return Err(error.context(crate::binding_flow::Failure(Code::PersistenceFailed)));
        }
        Ok((record, Some(receiver)))
    }
    pub fn change(
        &self,
        id: &str,
        edit: impl FnOnce(&mut BindingOperation) -> Result<()>,
    ) -> Result<BindingOperation> {
        let mut entries = self.entries.lock().unwrap();
        let entry = entries
            .active
            .get_mut(id)
            .context("binding operation not found")?;
        ensure!(
            entry.record.state == State::Running,
            "binding operation is no longer running"
        );
        edit(&mut entry.record)?;
        entry.record.sequence += 1;
        let record = entry.record.clone();
        self.persist(&entries)
            .context(crate::binding_flow::Failure(Code::PersistenceFailed))?;
        Ok(record)
    }
    pub fn cancel(&self, id: &str) -> Result<BindingOperation> {
        let entries = self.entries.lock().unwrap();
        ensure!(
            !entries.retired.contains_key(id),
            crate::binding_flow::Failure(Code::Superseded)
        );
        let entry = entries
            .active
            .get(id)
            .context("binding operation not found")?;
        if entry.record.state == State::Running {
            entry.cancel.send_replace(true);
        }
        Ok(entry.record.clone())
    }
    pub fn finish(&self, id: &str, state: State, code: Code, error: Option<&anyhow::Error>) {
        // Terminal or superseded operations must never recreate deleted diagnostics.
        let mut entries = self.entries.lock().unwrap();
        let Some(entry) = entries
            .active
            .get_mut(id)
            .filter(|e| e.record.state == State::Running)
        else {
            return;
        };
        if let Some(error) = error {
            let details: String = format!("{error:#}").chars().take(8192).collect();
            let _ = storage::write(
                &self
                    .path
                    .with_file_name(format!("binding-{id}.diagnostic.json")),
                &serde_json::json!({"code":code,"details":details,"execution":service_protocol::private_details(error)}),
            );
        }
        {
            let record = &mut entry.record;
            let stage = record
                .steps
                .iter()
                .find(|s| s.state == Step::Running)
                .map_or(BindingStage::Discover, |s| s.stage);
            record.state = state;
            record.diagnostic = Some(diagnostic(id, stage, code, error));
            let elapsed = now()
                .saturating_sub(record.started_unix_ms)
                .saturating_sub(record.steps.iter().map(|s| s.elapsed_ms).sum());
            for step in &mut record.steps {
                match step.state {
                    Step::Running => {
                        step.state = Step::Failed;
                        step.elapsed_ms = elapsed;
                    }
                    Step::Pending => step.state = Step::Skipped,
                    _ => {}
                }
            }
            record.sequence += 1;
        }
        let _ = self.persist(&entries);
    }
}

fn fingerprint(request: &BindingRequest) -> Result<String> {
    Ok(storage::digest(&serde_json::to_vec(request)?))
}
fn replay(entries: &Entries, request: &BindingRequest) -> Result<Option<BindingOperation>> {
    if let Some(entry) = entries.active.get(&request.request_id) {
        ensure!(
            fingerprint(request)? == fingerprint(&entry.record.request)?,
            "binding request identity conflict"
        );
        return Ok(Some(entry.record.clone()));
    }
    if let Some(retired) = entries.retired.get(&request.request_id) {
        ensure!(
            fingerprint(request)? == retired.fingerprint,
            "binding request identity conflict"
        );
        return Err(crate::binding_flow::Failure(Code::Superseded).into());
    }
    Ok(None)
}
fn retire(entries: &mut Entries, id: &str) -> Result<Entry> {
    let entry = entries
        .active
        .get(id)
        .context("binding operation not found")?;
    let retired = Retired {
        fingerprint: fingerprint(&entry.record.request)?,
        started_unix_ms: entry.record.started_unix_ms,
    };
    entries.retired.insert(id.into(), retired);
    if entries.retired.len() > 256 {
        let oldest = entries
            .retired
            .iter()
            .min_by_key(|(id, r)| (r.started_unix_ms, *id))
            .unwrap()
            .0
            .clone();
        entries.retired.remove(&oldest);
    }
    Ok(entries.active.remove(id).unwrap())
}

#[cfg(test)]
#[path = "bindings_tests.rs"]
mod tests;
