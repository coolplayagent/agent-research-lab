//! Adapter-only validation records. No collaboration identity or binding is created.
use crate::ResolvedAgent;
use contracts::{
    AdapterTestErrorCode, AdapterTestOperationState, AdapterTestStage, AdapterTestStepState,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterTestRequest {
    pub request_id: String,
    pub adapter_id: String,
    pub revision: u64,
    #[serde(default)]
    pub program: Option<PathBuf>,
    #[serde(default)]
    pub model: Option<String>,
}
impl AdapterTestRequest {
    pub fn validate(&self) -> anyhow::Result<()> {
        for id in [&self.request_id, &self.adapter_id] {
            collaboration_model::id(id)?;
        }
        if let Some(path) = &self.program {
            anyhow::ensure!(
                path.is_absolute() && path.as_os_str().len() <= 4096,
                "agent program must be an absolute path"
            );
        }
        if let Some(model) = &self.model {
            anyhow::ensure!(
                !model.is_empty() && model.len() <= 200 && !model.chars().any(char::is_control),
                "invalid model override"
            );
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterTestDiagnostic {
    pub code: AdapterTestErrorCode,
    pub stage: AdapterTestStage,
    pub diagnostic_id: String,
    pub os_code: Option<i32>,
    pub operation: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterTestStep {
    pub stage: AdapterTestStage,
    pub state: AdapterTestStepState,
    pub elapsed_ms: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterTestOperation {
    pub request: AdapterTestRequest,
    pub state: AdapterTestOperationState,
    pub sequence: u64,
    pub started_unix_ms: u64,
    pub steps: Vec<AdapterTestStep>,
    pub program: Option<ResolvedAgent>,
    pub diagnostic: Option<AdapterTestDiagnostic>,
    pub configuration_revision: Option<u64>,
    pub evidence: Option<AdapterTestEvidence>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterTestEvidence {
    pub request_id: String,
    pub executable_digest: String,
    pub adapter_digest: String,
    pub configuration_digest: String,
    pub input_digest: String,
    pub output_digest: String,
    pub sandbox_id: String,
    pub verified_unix_ms: u64,
}
