//! Host binding validation records, separate from goal and work lifecycles.
use contracts::{
    BindingErrorCode, BindingOperationState, BindingStage, BindingStepState, CodingAgent,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BindingRequest {
    pub request_id: String,
    pub person_id: String,
    pub adapter_id: String,
    pub revision: u64,
    #[serde(default)]
    pub program: Option<PathBuf>,
    #[serde(default)]
    pub model: Option<String>,
}
impl BindingRequest {
    pub fn validate(&self) -> anyhow::Result<()> {
        for id in [&self.request_id, &self.person_id, &self.adapter_id] {
            collaboration_model::id(id)?;
        }
        anyhow::ensure!(
            self.person_id != "operator",
            "operator cannot bind an executor"
        );
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
pub struct ResolvedAgent {
    pub agent: CodingAgent,
    pub executable: PathBuf,
    pub arguments: Vec<String>,
    pub version: String,
    pub digest: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BindingDiagnostic {
    pub code: BindingErrorCode,
    pub stage: BindingStage,
    pub diagnostic_id: String,
    pub os_code: Option<i32>,
    pub operation: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BindingStep {
    pub stage: BindingStage,
    pub state: BindingStepState,
    pub elapsed_ms: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BindingOperation {
    pub request: BindingRequest,
    pub state: BindingOperationState,
    pub sequence: u64,
    pub started_unix_ms: u64,
    pub steps: Vec<BindingStep>,
    pub program: Option<ResolvedAgent>,
    pub diagnostic: Option<BindingDiagnostic>,
    pub configuration_revision: Option<u64>,
    pub evidence: Option<BindingEvidence>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BindingEvidence {
    pub request_id: String,
    pub executable_digest: String,
    pub adapter_digest: String,
    pub input_digest: String,
    pub output_digest: String,
    pub sandbox_id: String,
    pub verified_unix_ms: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BindingProbeReceipt {
    pub execution: super::ExecutionReceipt,
    pub challenge_verified: bool,
}
