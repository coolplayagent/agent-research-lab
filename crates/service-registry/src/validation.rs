//! Shared execution mechanics. Each caller owns its lifecycle and persistence.
use crate::{Registry, binding_flow::Failure};
use anyhow::{Context, Result, ensure};
use contracts::{BindingErrorCode as Code, CodingAgent, ServiceKind};
use service_api::*;
use service_protocol as rpc;
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

fn saved_option(adapter: &Adapter, option: &str) -> Option<String> {
    saved_launch(adapter)?;
    adapter
        .command
        .get(1..)?
        .chunks_exact(2)
        .find(|pair| pair[0] == option)
        .map(|pair| pair[1].clone())
}
fn saved_launch(adapter: &Adapter) -> Option<(PathBuf, Vec<String>)> {
    let binary = std::path::Path::new(adapter.command.first()?);
    if binary.file_stem()? != "outcome-hub-agent" {
        return None;
    }
    if adapter.command.len() % 2 != 1 {
        return None;
    }
    let program = adapter
        .command
        .get(1..)?
        .chunks_exact(2)
        .find(|pair| pair[0] == "--program")?;
    let args = adapter
        .command
        .get(1..)?
        .chunks_exact(2)
        .filter(|pair| pair[0] == "--launcher-argument")
        .map(|pair| pair[1].clone())
        .collect();
    Some((PathBuf::from(&program[1]), args))
}
pub(super) async fn discover(
    adapter: &Adapter,
    program: Option<PathBuf>,
    model: Option<&str>,
) -> Result<ResolvedAgent> {
    let agent = adapter.agent;
    if agent == CodingAgent::Custom {
        ensure!(
            program.is_none() && model.is_none(),
            Failure(Code::ConfigurationInvalid)
        );
        let executable = std::path::PathBuf::from(
            adapter
                .command
                .first()
                .context("custom protocol command is missing")?,
        );
        let arguments = adapter.command[1..].to_vec();
        let digest = agent_adapter::executable_digest(&executable, &arguments)?;
        Ok(ResolvedAgent {
            agent,
            executable,
            arguments,
            digest,
            version: "json_stdio.v1".into(),
        })
    } else {
        let saved = saved_launch(adapter);
        tokio::task::spawn_blocking(move || {
            if program.is_none()
                && let Some((executable, arguments)) = saved
            {
                agent_adapter::rediscover(agent, executable, arguments)
            } else {
                agent_adapter::discover(agent, program.as_deref())
            }
        })
        .await?
    }
}
impl Registry {
    pub(super) async fn validation_runtime(
        &self,
        config: &Configuration,
        adapter: &Adapter,
    ) -> Result<(Service, Service)> {
        self.prepare_execution(config, &adapter.executor_id)
            .await
            .map_err(|error| error.context(Failure(Code::ServiceUnavailable)))?;
        let executor = config
            .service(&adapter.executor_id, ServiceKind::Executor)?
            .clone();
        let sandbox = config
            .service(
                executor
                    .sandbox_id
                    .as_deref()
                    .context("executor sandbox is missing")?,
                ServiceKind::Sandbox,
            )?
            .clone();
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if service_api::probe(&executor.endpoint, ServiceKind::Executor)
                .await
                .is_ok()
            {
                break;
            }
            ensure!(Instant::now() < deadline, Failure(Code::ServiceUnavailable));
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        service_api::probe(&sandbox.endpoint, ServiceKind::Sandbox)
            .await
            .map_err(|e| e.context(Failure(Code::IsolationUnavailable)))?;
        Ok((executor, sandbox))
    }
}
pub(super) async fn configure(
    adapter: &mut Adapter,
    resolved: &ResolvedAgent,
    model: Option<String>,
) -> Result<()> {
    let agent = adapter.agent;
    let model = model.or_else(|| saved_option(adapter, "--model"));
    if agent != CodingAgent::Custom {
        let configuration_digest = agent_adapter::configuration_digest(agent)?;
        let bridge = std::env::current_exe()?.with_file_name(if cfg!(windows) {
            "outcome-hub-agent.exe"
        } else {
            "outcome-hub-agent"
        });
        ensure!(bridge.is_file(), "packaged agent bridge is missing");
        adapter.command = vec![
            bridge.to_string_lossy().into_owned(),
            "--agent".into(),
            agent.to_string(),
            "--program".into(),
            resolved.executable.to_string_lossy().into_owned(),
            "--digest".into(),
            resolved.digest.clone(),
        ];
        for argument in &resolved.arguments {
            adapter
                .command
                .extend(["--launcher-argument".into(), argument.clone()]);
        }
        adapter
            .command
            .extend(["--configuration-digest".into(), configuration_digest]);
        let selected_model = if agent == CodingAgent::DeepseekHarness && model.is_none() {
            let resolved = resolved.clone();
            Some(
                tokio::task::spawn_blocking(move || agent_adapter::selected_model(&resolved))
                    .await??,
            )
        } else {
            model.clone()
        };
        if let Some(model) = &selected_model {
            adapter.command.extend(["--model".into(), model.clone()]);
        }
        adapter.enabled = true;
        adapter.env_names = agent_adapter::environment_names(agent);
        adapter.read_only_paths = agent_adapter::runtime_paths(&resolved)?;
        adapter.read_only_paths.push(bridge);
    }
    adapter.enabled = true;
    adapter.validate()?;
    Ok(())
}
pub(super) fn input(id: &str, person: &str, purpose: &str) -> AgentInput {
    AgentInput {
        protocol_version: rpc::VERSION,
        request_id: id.into(),
        person_id: person.into(),
        goal_id: purpose.into(),
        work_id: purpose.into(),
        attempt: 1,
        objective: "Validate the installed coding agent with one bounded workspace task.".into(),
        acceptance: "The host verifies the resulting artifact.".into(),
        instruction: String::new(),
        messages: vec![],
    }
}
pub(super) async fn execute(
    adapter: &Adapter,
    resolved: &ResolvedAgent,
    executor: Service,
    sandbox: Service,
    input: AgentInput,
) -> Result<BindingProbeReceipt> {
    let agent = adapter.agent;
    let request_id = input.request_id.clone();
    let expected_input = input.clone();
    let receipt: BindingProbeReceipt = rpc::call(
        &executor.endpoint,
        &ExecutorRequest::Validate {
            adapter: adapter.clone(),
            sandbox: sandbox.endpoint,
            input: Box::new(input),
            seconds: adapter.max_seconds.min(90),
        },
        Duration::from_secs(95),
    )
    .await
    .map_err(|error| {
        let code = execution_failure_code(agent, &error);
        error.context(Failure(code))
    })?;
    receipt
        .execution
        .output
        .validate(&expected_input)
        .map_err(|error| error.context(Failure(Code::InvalidResponse)))?;
    ensure!(
        receipt.challenge_verified && receipt.execution.output.succeeded,
        Failure(Code::VerificationFailed)
    );
    ensure!(
        receipt.execution.adapter_sha256 == storage::digest(&serde_json::to_vec(adapter)?)
            && receipt.execution.output.request_id == request_id,
        Failure(Code::InvalidResponse)
    );
    ensure!(
        agent_adapter::executable_digest(&resolved.executable, &resolved.arguments)?
            == resolved.digest,
        Failure(Code::ProgramInvalid)
    );
    if let Some(pair) = adapter
        .command
        .windows(2)
        .find(|pair| pair[0] == "--configuration-digest")
    {
        ensure!(
            agent_adapter::configuration_digest(agent)? == pair[1],
            Failure(Code::ConfigurationInvalid)
        );
    }
    Ok(receipt)
}

fn execution_failure_code(agent: CodingAgent, error: &anyhow::Error) -> Code {
    if agent == CodingAgent::Codex
        && let Some(details) = rpc::private_details(error)
        && ((details.contains("codex_login::auth") && details.contains("refresh_token_reused"))
            || (details.contains("401") && details.contains("token_expired")))
    {
        // Map known provider failures to a public code, never public stderr.
        // Unknown failures retain the failed validation state and private details.
        return Code::AuthenticationRequired;
    }
    Code::VerificationFailed
}

#[cfg(test)]
#[path = "validation_tests.rs"]
mod tests;
