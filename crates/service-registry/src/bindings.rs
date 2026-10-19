//! Durable, idempotent validation operations. A restart never implies success.
use anyhow::{Context, Result, ensure};
use contracts::{
    BindingErrorCode as Code, BindingOperationState as State, BindingStage,
    BindingStepState as Step,
};
use service_api::*;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Mutex,
};
use tokio::sync::watch;

pub(super) struct Operations {
    path: PathBuf,
    entries: Mutex<BTreeMap<String, Entry>>,
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
        let records: Vec<BindingOperation> = if path.exists() {
            storage::read(&path)?
        } else {
            vec![]
        };
        ensure!(records.len() <= 256, "binding journal exceeds bound");
        let mut entries = BTreeMap::new();
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
            entries.insert(
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
        operations.persist(&operations.entries.lock().unwrap())?;
        Ok(operations)
    }
    fn persist(&self, entries: &BTreeMap<String, Entry>) -> Result<()> {
        storage::write(
            &self.path,
            &entries.values().map(|e| &e.record).collect::<Vec<_>>(),
        )
    }
    pub fn list(&self) -> Vec<BindingOperation> {
        self.entries
            .lock()
            .unwrap()
            .values()
            .map(|e| e.record.clone())
            .collect()
    }
    pub fn get(&self, id: &str) -> Result<BindingOperation> {
        self.entries
            .lock()
            .unwrap()
            .get(id)
            .map(|e| e.record.clone())
            .context("binding operation not found")
    }
    pub fn begin(
        &self,
        request: BindingRequest,
    ) -> Result<(BindingOperation, Option<watch::Receiver<bool>>)> {
        let mut entries = self.entries.lock().unwrap();
        if let Some(entry) = entries.get(&request.request_id) {
            ensure!(
                serde_json::to_value(&request)? == serde_json::to_value(&entry.record.request)?,
                "binding request identity conflict"
            );
            return Ok((entry.record.clone(), None));
        }
        let running = entries
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
        if entries.len() >= 256 {
            let oldest = entries
                .iter()
                .filter(|(_, e)| e.record.state != State::Running)
                .min_by_key(|(_, e)| e.record.started_unix_ms)
                .map(|(id, _)| id.clone())
                .context("binding journal is full")?;
            entries.remove(&oldest);
            let _ = std::fs::remove_file(
                self.path
                    .with_file_name(format!("binding-{oldest}.diagnostic.json")),
            );
        }
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
        entries.insert(
            id.clone(),
            Entry {
                record: record.clone(),
                cancel,
            },
        );
        if let Err(error) = self.persist(&entries) {
            entries.remove(&id);
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
        let entry = entries.get_mut(id).context("binding operation not found")?;
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
        let entry = entries.get(id).context("binding operation not found")?;
        if entry.record.state == State::Running {
            entry.cancel.send_replace(true);
        }
        Ok(entry.record.clone())
    }
    pub fn finish(&self, id: &str, state: State, code: Code, error: Option<&anyhow::Error>) {
        if let Some(error) = error {
            let details: String = format!("{error:#}").chars().take(8192).collect();
            let _ = storage::write(
                &self
                    .path
                    .with_file_name(format!("binding-{id}.diagnostic.json")),
                &serde_json::json!({"code":code,"details":details}),
            );
        }
        let _ = self.change(id, |record| {
            let stage = record
                .steps
                .iter()
                .find(|s| s.state == Step::Running)
                .map_or(BindingStage::Discover, |s| s.stage);
            record.state = state;
            record.diagnostic = Some(diagnostic(id, stage, code, error));
            for step in &mut record.steps {
                match step.state {
                    Step::Running => step.state = Step::Failed,
                    Step::Pending => step.state = Step::Skipped,
                    _ => {}
                }
            }
            Ok(())
        });
    }
}

#[cfg(test)]
#[path = "bindings_tests.rs"]
mod tests;
