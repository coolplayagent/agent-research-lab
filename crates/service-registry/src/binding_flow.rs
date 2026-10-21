use crate::{Registry, bindings};
use anyhow::{Context, Result, ensure};
use contracts::{
    BindingErrorCode as Code, BindingOperationState as State, BindingStage as Stage,
    BindingStepState as Step, CodingAgent, ServiceKind,
};
use service_api::*;
use service_protocol as rpc;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

impl Registry {
    pub fn binding_operations(&self) -> Vec<BindingOperation> {
        self.bindings.list()
    }
    pub fn binding_operation(&self, id: &str) -> Result<BindingOperation> {
        self.bindings.get(id)
    }
    pub fn cancel_binding(&self, id: &str) -> Result<BindingOperation> {
        self.bindings.cancel(id)
    }
    pub fn start_binding(self: &Arc<Self>, request: BindingRequest) -> Result<BindingOperation> {
        request.validate()?;
        // Existing request IDs are checked before revision validation: a lost POST response
        // can be retried after the atomic configuration commit without another model call.
        if let Some(existing) = self.bindings.replay(&request)? {
            return Ok(existing);
        }
        let config = self.configuration();
        ensure!(
            config.revision == request.revision,
            Failure(Code::RevisionConflict)
        );
        ensure!(
            config.adapters.iter().any(|a| a.id == request.adapter_id),
            "binding adapter is missing"
        );
        let (record, cancel) = self.bindings.begin(request.clone())?;
        if let Some(mut cancel) = cancel {
            let registry = self.clone();
            tokio::spawn(async move {
                let id = &request.request_id;
                let cancellation = cancel.clone();
                let result = tokio::select! {
                    biased;
                    _ = cancel.changed() => { registry.bindings.finish(id, State::Cancelled, Code::Cancelled, None); return; }
                    result = tokio::time::timeout(Duration::from_secs(120), registry.validate_binding(&request, &cancellation)) => result,
                };
                match result {
                    Err(_) => registry
                        .bindings
                        .finish(id, State::Failed, Code::Timeout, None),
                    Ok(Err(error)) => {
                        let stage = registry
                            .bindings
                            .get(id)
                            .ok()
                            .and_then(|r| r.steps.into_iter().find(|s| s.state == Step::Running))
                            .map_or(Stage::Discover, |s| s.stage);
                        let code = error.downcast_ref::<Failure>().map_or(
                            match stage {
                                Stage::Discover => agent_adapter::discovery_failure_code(&error)
                                    .unwrap_or(Code::ProgramMissing),
                                Stage::Runtime => Code::ServiceUnavailable,
                                Stage::Configuration => Code::ConfigurationInvalid,
                                Stage::Execute => Code::VerificationFailed,
                                Stage::Persist => Code::PersistenceFailed,
                            },
                            |f| f.0,
                        );
                        registry
                            .bindings
                            .finish(id, State::Failed, code, Some(&error));
                    }
                    Ok(Ok(())) => {}
                }
            });
        }
        Ok(record)
    }
    fn stage(&self, id: &str, stage: Stage) -> Result<Instant> {
        self.bindings.change(id, |r| {
            r.steps
                .iter_mut()
                .find(|s| s.stage == stage)
                .context("binding stage missing")?
                .state = Step::Running;
            Ok(())
        })?;
        Ok(Instant::now())
    }
    fn passed(&self, id: &str, stage: Stage, started: Instant) -> Result<()> {
        self.bindings.change(id, |r| {
            let step = r
                .steps
                .iter_mut()
                .find(|s| s.stage == stage)
                .context("binding stage missing")?;
            step.state = Step::Passed;
            step.elapsed_ms = started.elapsed().as_millis() as u64;
            Ok(())
        })?;
        Ok(())
    }
    async fn validate_binding(
        &self,
        request: &BindingRequest,
        cancelled: &tokio::sync::watch::Receiver<bool>,
    ) -> Result<()> {
        let id = &request.request_id;
        let mut config = self.configuration();
        ensure!(
            config.revision == request.revision,
            Failure(Code::RevisionConflict)
        );
        let index = config
            .adapters
            .iter()
            .position(|a| a.id == request.adapter_id)
            .context("binding adapter is missing")?;
        let agent = config.adapters[index].agent;
        let started = self.stage(id, Stage::Discover)?;
        let program = request.program.clone();
        let resolved = if agent == CodingAgent::Custom {
            ensure!(
                program.is_none() && request.model.is_none(),
                Failure(Code::ConfigurationInvalid)
            );
            let adapter = &config.adapters[index];
            let executable = std::path::PathBuf::from(
                adapter
                    .command
                    .first()
                    .context("custom protocol command is missing")?,
            );
            let arguments = adapter.command[1..].to_vec();
            let digest = agent_adapter::executable_digest(&executable, &arguments)?;
            ResolvedAgent {
                agent,
                executable,
                arguments,
                digest,
                version: "json_stdio.v1".into(),
            }
        } else {
            tokio::task::spawn_blocking(move || agent_adapter::discover(agent, program.as_deref()))
                .await??
        };
        self.bindings.change(id, |r| {
            r.program = Some(resolved.clone());
            Ok(())
        })?;
        self.passed(id, Stage::Discover, started)?;

        let started = self.stage(id, Stage::Runtime)?;
        self.prepare_execution(&config, &config.adapters[index].executor_id)
            .await
            .map_err(|error| error.context(Failure(Code::ServiceUnavailable)))?;
        let executor = config
            .service(&config.adapters[index].executor_id, ServiceKind::Executor)?
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
        self.passed(id, Stage::Runtime, started)?;

        let started = self.stage(id, Stage::Configuration)?;
        let adapter = &mut config.adapters[index];
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
            let selected_model = if agent == CodingAgent::DeepseekHarness && request.model.is_none()
            {
                let resolved = resolved.clone();
                Some(
                    tokio::task::spawn_blocking(move || agent_adapter::selected_model(&resolved))
                        .await??,
                )
            } else {
                request.model.clone()
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
        self.passed(id, Stage::Configuration, started)?;

        let started = self.stage(id, Stage::Execute)?;
        let input = AgentInput {
            protocol_version: rpc::VERSION,
            request_id: id.clone(),
            person_id: request.person_id.clone(),
            goal_id: "binding-validation".into(),
            work_id: "binding-validation".into(),
            attempt: 1,
            objective: "Validate the installed coding agent with one bounded workspace task."
                .into(),
            acceptance: "The host verifies the resulting artifact.".into(),
            instruction: String::new(),
            messages: vec![],
        };
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
        .await?;
        ensure!(
            receipt.challenge_verified && receipt.execution.output.succeeded,
            Failure(Code::VerificationFailed)
        );
        ensure!(
            receipt.execution.adapter_sha256 == storage::digest(&serde_json::to_vec(adapter)?)
                && receipt.execution.output.request_id == *id,
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
        self.passed(id, Stage::Execute, started)?;

        let started = self.stage(id, Stage::Persist)?;
        ensure!(!*cancelled.borrow(), Failure(Code::Cancelled));
        ensure!(
            self.configuration().revision == request.revision,
            Failure(Code::RevisionConflict)
        );
        let evidence = BindingEvidence {
            request_id: id.clone(),
            executable_digest: resolved.digest,
            adapter_digest: receipt.execution.adapter_sha256,
            input_digest: receipt.execution.input_sha256,
            output_digest: receipt.execution.output_sha256,
            sandbox_id: receipt.execution.sandbox_id,
            verified_unix_ms: bindings::now(),
        };
        config
            .bindings
            .insert(request.person_id.clone(), request.adapter_id.clone());
        config
            .binding_evidence
            .insert(request.person_id.clone(), evidence.clone());
        // Configuration and evidence have one atomic commit point. Recovery reconciles
        // a successful commit if writing the subsequent operation journal fails.
        self.bindings.change(id, |r| {
            let saved = self.save(config)?;
            r.state = State::Succeeded;
            r.evidence = Some(evidence);
            r.configuration_revision = Some(saved.revision);
            let step = r
                .steps
                .iter_mut()
                .find(|s| s.stage == Stage::Persist)
                .context("binding stage missing")?;
            step.state = Step::Passed;
            step.elapsed_ms = started.elapsed().as_millis() as u64;
            Ok(())
        })?;
        Ok(())
    }
}
#[derive(Debug)]
pub(super) struct Failure(pub Code);
impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl std::error::Error for Failure {}

pub fn binding_failure_code(error: &anyhow::Error) -> Code {
    error
        .downcast_ref::<Failure>()
        .map_or(Code::ConfigurationInvalid, |failure| failure.0)
}
