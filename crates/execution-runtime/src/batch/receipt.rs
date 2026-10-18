use super::fail;
use anyhow::Result;
use contracts::{
    BatchEntryRegistrationState as EntryState, BatchRegistrationErrorCode as Code,
    BatchRegistrationPhase as Phase, BatchRegistrationState as State,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub task_id: String,
    pub state: EntryState,
    pub attempt: Option<u32>,
    pub run_id: Option<String>,
    pub worktree: Option<PathBuf>,
    pub proof: Option<workflow::RegistrationProof>,
    pub frozen_input_digest: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub schema_version: u32,
    pub batch_id: String,
    pub request_digest: String,
    pub observed_request_sha256: String,
    pub plan_digest: Option<String>,
    pub config_digest: String,
    pub input_identity: Option<String>,
    pub collected_inputs_digest: Option<String>,
    pub started_at_ms: u64,
    pub finished_at_ms: Option<u64>,
    pub max_seconds: u64,
    pub state: State,
    pub phase: Phase,
    pub error_code: Option<Code>,
    pub entries: Vec<Observation>,
    pub receipt_path: PathBuf,
    pub receipt_persisted: bool,
}

#[derive(Debug)]
pub struct Failure {
    pub code: Code,
    pub message: String,
    pub registration: Option<Box<Receipt>>,
}
impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for Failure {}

impl Receipt {
    pub(super) fn save(&mut self) -> Result<()> {
        self.receipt_persisted = true;
        if let Err(error) = storage::write(&self.receipt_path, self) {
            self.receipt_persisted = false;
            return Err(fail(Code::ReceiptPersistenceFailure, error));
        }
        Ok(())
    }
}
