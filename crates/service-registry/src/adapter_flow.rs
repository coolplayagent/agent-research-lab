use crate::{Registry, validation};
use anyhow::{Context, Result, ensure};
use contracts::{
    AdapterTestErrorCode as Code, AdapterTestOperationState as State, AdapterTestStage as Stage,
    AdapterTestStepState as Step,
};
use service_api::*;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Debug)]
pub(super) struct Failure(pub Code);
impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl std::error::Error for Failure {}
pub fn adapter_test_failure_code(error: &anyhow::Error) -> Code {
    error
        .downcast_ref::<Failure>()
        .map_or(Code::ConfigurationInvalid, |failure| failure.0)
}
fn execution_code(code: contracts::BindingErrorCode) -> Code {
    match code {
        contracts::BindingErrorCode::ProgramMissing => Code::ProgramMissing,
        contracts::BindingErrorCode::ProgramInvalid => Code::ProgramInvalid,
        contracts::BindingErrorCode::LaunchFailed => Code::LaunchFailed,
        contracts::BindingErrorCode::ConfigurationInvalid => Code::ConfigurationInvalid,
        contracts::BindingErrorCode::ServiceUnavailable => Code::ServiceUnavailable,
        contracts::BindingErrorCode::IsolationUnavailable => Code::IsolationUnavailable,
        contracts::BindingErrorCode::VerificationFailed => Code::VerificationFailed,
        contracts::BindingErrorCode::AuthenticationRequired => Code::AuthenticationRequired,
        contracts::BindingErrorCode::InvalidResponse => Code::InvalidResponse,
        contracts::BindingErrorCode::Timeout => Code::Timeout,
        contracts::BindingErrorCode::Cancelled => Code::Cancelled,
        contracts::BindingErrorCode::RevisionConflict => Code::RevisionConflict,
        contracts::BindingErrorCode::PersistenceFailed => Code::PersistenceFailed,
        contracts::BindingErrorCode::Interrupted => Code::Interrupted,
        contracts::BindingErrorCode::Busy => Code::Busy,
        contracts::BindingErrorCode::ValidationRequired => Code::ValidationRequired,
        contracts::BindingErrorCode::Superseded => Code::Superseded,
    }
}
impl Registry {
    pub fn adapter_test_operations(&self) -> Vec<AdapterTestOperation> {
        self.adapter_tests.list()
    }
    pub fn adapter_test_operation(&self, id: &str) -> Result<AdapterTestOperation> {
        self.adapter_tests.get(id)
    }
    pub fn cancel_adapter_test(&self, id: &str) -> Result<AdapterTestOperation> {
        self.adapter_tests.cancel(id)
    }
    pub fn start_adapter_test(
        self: &Arc<Self>,
        request: AdapterTestRequest,
    ) -> Result<AdapterTestOperation> {
        request.validate()?;
        if let Some(existing) = self.adapter_tests.replay(&request)? {
            return Ok(existing);
        }
        let config = self.configuration();
        ensure!(
            config.revision == request.revision,
            Failure(Code::RevisionConflict)
        );
        ensure!(
            config.adapters.iter().any(|a| a.id == request.adapter_id),
            Failure(Code::ConfigurationInvalid)
        );
        let permit = self
            .validation_slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| Failure(Code::Busy))?;
        let (record, cancel) = self.adapter_tests.begin(request.clone())?;
        if let Some(mut cancel) = cancel {
            let registry = self.clone();
            tokio::spawn(async move {
                let _permit = permit;
                let id = &request.request_id;
                let cancellation = cancel.clone();
                let result = tokio::select! {
                    biased;
                    _ = cancel.changed() => { registry.adapter_tests.finish(id, State::Cancelled, Code::Cancelled, None); return; }
                    result = tokio::time::timeout(Duration::from_secs(120), registry.validate_adapter(&request, &cancellation)) => result,
                };
                match result {
                    Err(_) => registry
                        .adapter_tests
                        .finish(id, State::Failed, Code::Timeout, None),
                    Ok(Err(error)) => {
                        let stage = registry
                            .adapter_tests
                            .get(id)
                            .ok()
                            .and_then(|r| r.steps.into_iter().find(|s| s.state == Step::Running))
                            .map_or(Stage::Discover, |s| s.stage);
                        let code = error
                            .downcast_ref::<Failure>()
                            .map(|f| f.0)
                            .or_else(|| {
                                error
                                    .downcast_ref::<crate::binding_flow::Failure>()
                                    .map(|f| execution_code(f.0))
                            })
                            .or_else(|| {
                                agent_adapter::discovery_failure_code(&error).map(execution_code)
                            })
                            .unwrap_or(match stage {
                                Stage::Discover => Code::ProgramMissing,
                                Stage::Runtime => Code::ServiceUnavailable,
                                Stage::Configuration => Code::ConfigurationInvalid,
                                Stage::Execute => Code::VerificationFailed,
                                Stage::Persist => Code::PersistenceFailed,
                            });
                        registry
                            .adapter_tests
                            .finish(id, State::Failed, code, Some(&error));
                    }
                    Ok(Ok(())) => {}
                }
            });
        }
        Ok(record)
    }
    fn adapter_stage(&self, id: &str, stage: Stage) -> Result<Instant> {
        self.adapter_tests.change(id, |r| {
            r.steps
                .iter_mut()
                .find(|s| s.stage == stage)
                .context("adapter test stage missing")?
                .state = Step::Running;
            Ok(())
        })?;
        Ok(Instant::now())
    }
    fn adapter_passed(&self, id: &str, stage: Stage, started: Instant) -> Result<()> {
        self.adapter_tests.change(id, |r| {
            let step = r
                .steps
                .iter_mut()
                .find(|s| s.stage == stage)
                .context("adapter test stage missing")?;
            step.state = Step::Passed;
            step.elapsed_ms = started.elapsed().as_millis() as u64;
            Ok(())
        })?;
        Ok(())
    }
    async fn validate_adapter(
        &self,
        request: &AdapterTestRequest,
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
            .context("adapter is missing")?;
        let started = self.adapter_stage(id, Stage::Discover)?;
        let resolved = validation::discover(
            &config.adapters[index],
            request.program.clone(),
            request.model.as_deref(),
        )
        .await?;
        self.adapter_tests.change(id, |r| {
            r.program = Some(resolved.clone());
            Ok(())
        })?;
        self.adapter_passed(id, Stage::Discover, started)?;
        let started = self.adapter_stage(id, Stage::Runtime)?;
        let (executor, sandbox) = self
            .validation_runtime(&config, &config.adapters[index])
            .await?;
        self.adapter_passed(id, Stage::Runtime, started)?;
        let started = self.adapter_stage(id, Stage::Configuration)?;
        validation::configure(
            &mut config.adapters[index],
            &resolved,
            request.model.clone(),
        )
        .await?;
        self.adapter_passed(id, Stage::Configuration, started)?;
        let started = self.adapter_stage(id, Stage::Execute)?;
        // Protocol context only: this identity is never registered in collaboration storage.
        let input = validation::input(id, "adapter-test", "adapter-test");
        let receipt =
            validation::execute(&config.adapters[index], &resolved, executor, sandbox, input)
                .await?;
        self.adapter_passed(id, Stage::Execute, started)?;
        let started = self.adapter_stage(id, Stage::Persist)?;
        let evidence = AdapterTestEvidence {
            request_id: id.clone(),
            executable_digest: resolved.digest,
            configuration_digest: storage::digest(
                config
                    .adapter_test_configuration(&request.adapter_id)?
                    .as_bytes(),
            ),
            adapter_digest: receipt.execution.adapter_sha256,
            input_digest: receipt.execution.input_sha256,
            output_digest: receipt.execution.output_sha256,
            sandbox_id: receipt.execution.sandbox_id,
            verified_unix_ms: crate::bindings::now(),
        };
        config
            .adapter_test_evidence
            .insert(request.adapter_id.clone(), evidence.clone());
        self.adapter_tests.change(id, |r| {
            ensure!(!*cancelled.borrow(), Failure(Code::Cancelled));
            let saved = self.save(config).map_err(|error| {
                let code = if crate::binding_failure_code(&error)
                    == contracts::BindingErrorCode::RevisionConflict
                {
                    Code::RevisionConflict
                } else {
                    Code::PersistenceFailed
                };
                error.context(Failure(code))
            })?;
            r.state = State::Succeeded;
            r.evidence = Some(evidence);
            r.configuration_revision = Some(saved.revision);
            let step = r
                .steps
                .iter_mut()
                .find(|s| s.stage == Stage::Persist)
                .context("adapter test stage missing")?;
            step.state = Step::Passed;
            step.elapsed_ms = started.elapsed().as_millis() as u64;
            Ok(())
        })?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "adapter_flow_tests.rs"]
mod tests;
