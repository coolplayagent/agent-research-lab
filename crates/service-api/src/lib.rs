//! Versioned service and coding-agent contracts. Implementations live in independent processes.
mod registry;
use anyhow::{Result, ensure};
use contracts::{AgentProtocol, CodingAgent, ServiceKind};
pub use registry::*;
use serde::{Deserialize, Serialize};
use service_protocol as rpc;
use std::{collections::BTreeMap, path::PathBuf, time::Duration};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Adapter {
    pub id: String,
    pub agent: CodingAgent,
    pub protocol: AgentProtocol,
    pub enabled: bool,
    pub executor_id: String,
    pub command: Vec<String>,
    pub env_names: Vec<String>,
    pub read_only_paths: Vec<PathBuf>,
    pub network: bool,
    pub max_seconds: u64,
}
impl Adapter {
    pub fn validate(&self) -> Result<()> {
        crystal::id(&self.id)?;
        crystal::id(&self.executor_id)?;
        ensure!(
            (1..=3600).contains(&self.max_seconds),
            "adapter time limit must be 1..3600 seconds"
        );
        ensure!(
            !self.enabled || !self.command.is_empty(),
            "enabled adapter requires a JSON protocol command"
        );
        ensure!(
            self.command.len() <= 64
                && self
                    .command
                    .iter()
                    .all(|s| s.len() <= 4096 && !s.contains('\0')),
            "invalid adapter command"
        );
        if let Some(program) = self.command.first() {
            ensure!(
                std::path::Path::new(program).is_absolute(),
                "adapter command must be absolute"
            );
        }
        ensure!(
            self.env_names.len() <= 32
                && self.env_names.iter().all(|name| !name.is_empty()
                    && name.len() <= 80
                    && name
                        .bytes()
                        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == b'_')
                    && ![
                        "LD_PRELOAD",
                        "LD_LIBRARY_PATH",
                        "BASH_ENV",
                        "ENV",
                        "NODE_OPTIONS",
                        "PYTHONPATH"
                    ]
                    .contains(&name.as_str())),
            "invalid environment name allowlist"
        );
        ensure!(
            self.read_only_paths.len() <= 16,
            "too many read-only mounts"
        );
        for path in &self.read_only_paths {
            ensure!(
                path.is_absolute()
                    && path.components().count() > 2
                    && !path
                        .components()
                        .any(|p| matches!(p, std::path::Component::ParentDir)),
                "mount must be an absolute specific path"
            );
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentInput {
    pub protocol_version: u32,
    pub request_id: String,
    pub person_id: String,
    pub goal_id: String,
    pub work_id: String,
    pub attempt: u32,
    pub objective: String,
    pub acceptance: String,
    pub instruction: String,
    pub messages: Vec<crystal::Message>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentOutput {
    pub protocol_version: u32,
    pub request_id: String,
    pub summary: String,
    pub succeeded: bool,
    #[serde(default)]
    pub evidence: serde_json::Value,
}
impl AgentOutput {
    pub fn validate(&self, input: &AgentInput) -> Result<()> {
        ensure!(
            self.protocol_version == rpc::VERSION && self.request_id == input.request_id,
            "adapter response identity or version mismatch"
        );
        ensure!(
            !self.summary.trim().is_empty() && self.summary.len() <= 8000,
            "invalid adapter summary"
        );
        ensure!(
            serde_json::to_vec(&self.evidence)?.len() <= 8192,
            "adapter evidence exceeds bound"
        );
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionReceipt {
    pub output: AgentOutput,
    pub agent: CodingAgent,
    pub adapter_id: String,
    pub input_sha256: String,
    pub adapter_sha256: String,
    pub output_sha256: String,
    pub sandbox_id: String,
    pub elapsed_ms: u64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExecutorRequest {
    Probe,
    Execute {
        adapter: Adapter,
        sandbox: PathBuf,
        input: Box<AgentInput>,
        seconds: u64,
    },
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum SandboxRequest {
    Probe,
    Run {
        command: Vec<String>,
        env_names: Vec<String>,
        read_only_paths: Vec<PathBuf>,
        network: bool,
        input: String,
        seconds: u64,
    },
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SandboxOutput {
    pub stdout: String,
    pub elapsed_ms: u64,
    pub sandbox_id: String,
}
pub async fn probe(path: &std::path::Path, kind: ServiceKind) -> Result<rpc::ServiceInfo> {
    let info: rpc::ServiceInfo = rpc::call(
        path,
        &serde_json::json!({"operation":"probe"}),
        Duration::from_secs(8),
    )
    .await?;
    ensure!(
        info.protocol_version == rpc::VERSION && info.kind == kind,
        "service protocol or kind mismatch"
    );
    let required = match kind {
        ServiceKind::Storage => "collaboration.v1",
        ServiceKind::Executor => "json_stdio.v1",
        ServiceKind::Sandbox => "sandbox.run.v1",
    };
    ensure!(
        info.capabilities.iter().any(|c| c == required),
        "service capability mismatch"
    );
    ensure!(info.ready, "service is not ready");
    Ok(info)
}
pub type Bindings = BTreeMap<String, String>;
