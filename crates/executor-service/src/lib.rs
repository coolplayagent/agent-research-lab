//! Stateless coding-agent execution over the versioned sandbox protocol.
use anyhow::{Result, ensure};
use contracts::ServiceKind;
use serde_json::{Value, json};
use service_api::{ExecutionReceipt, ExecutorRequest, SandboxRequest};
use service_protocol as rpc;
use std::time::Duration;
pub async fn execute(request: ExecutorRequest) -> Result<Value> {
    match request {
        ExecutorRequest::Probe => Ok(json!(info())),
        ExecutorRequest::Execute {
            adapter,
            sandbox,
            input,
            seconds,
        } => {
            adapter.validate()?;
            ensure!(adapter.enabled, "adapter is disabled");
            ensure!(
                input.protocol_version == rpc::VERSION,
                "unsupported agent input protocol version"
            );
            ensure!(
                (1..=adapter.max_seconds).contains(&seconds),
                "execution time exceeds adapter limit"
            );
            rpc::endpoint(&sandbox)?;
            service_api::probe(&sandbox, ServiceKind::Sandbox).await?;
            let encoded = serde_json::to_string(&input)?;
            ensure!(encoded.len() <= 256 * 1024, "agent input exceeds bound");
            let input_sha256 = storage::digest(encoded.as_bytes());
            let adapter_sha256 = storage::digest(&serde_json::to_vec(&adapter)?);
            let output: service_api::SandboxOutput = rpc::call(
                &sandbox,
                &SandboxRequest::Run {
                    command: adapter.command.clone(),
                    env_names: adapter.env_names.clone(),
                    read_only_paths: adapter.read_only_paths.clone(),
                    network: adapter.network,
                    input: encoded,
                    seconds,
                },
                Duration::from_secs(seconds + 10),
            )
            .await?;
            let result: service_api::AgentOutput = serde_json::from_str(&output.stdout)?;
            result.validate(&input)?;
            Ok(json!(ExecutionReceipt {
                output_sha256: storage::digest(&serde_json::to_vec(&result)?),
                output: result,
                agent: adapter.agent,
                adapter_id: adapter.id,
                input_sha256,
                adapter_sha256,
                sandbox_id: output.sandbox_id,
                elapsed_ms: output.elapsed_ms
            }))
        }
    }
}
fn info() -> rpc::ServiceInfo {
    rpc::ServiceInfo {
        protocol_version: rpc::VERSION,
        service_id: "ai-im-executor".into(),
        kind: ServiceKind::Executor,
        capabilities: vec!["json_stdio.v1".into()],
        ready: true,
    }
}

#[cfg(test)]
mod tests;
