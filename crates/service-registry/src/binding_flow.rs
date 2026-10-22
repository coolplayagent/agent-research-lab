use crate::{Registry, bindings};
use anyhow::{Context, Result, ensure};
use contracts::{
    BindingErrorCode as Code, BindingOperationState as State, BindingStage as Stage,
    BindingStepState as Step,
};
use service_api::*;
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
        let permit = self
            .validation_slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| Failure(Code::Busy))?;
        let (record, cancel) = self.bindings.begin(request.clone())?;
        if let Some(mut cancel) = cancel {
            let registry = self.clone();
            tokio::spawn(async move {
                let _permit = permit;
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
        let started = self.stage(id, Stage::Discover)?;
        let resolved = crate::validation::discover(
            &config.adapters[index],
            request.program.clone(),
            request.model.as_deref(),
        )
        .await?;
        self.bindings.change(id, |r| {
            r.program = Some(resolved.clone());
            Ok(())
        })?;
        self.passed(id, Stage::Discover, started)?;
        let started = self.stage(id, Stage::Runtime)?;
        let (executor, sandbox) = self
            .validation_runtime(&config, &config.adapters[index])
            .await?;
        self.passed(id, Stage::Runtime, started)?;
        let started = self.stage(id, Stage::Configuration)?;
        crate::validation::configure(
            &mut config.adapters[index],
            &resolved,
            request.model.clone(),
        )
        .await?;
        self.passed(id, Stage::Configuration, started)?;
        let started = self.stage(id, Stage::Execute)?;
        let input = crate::validation::input(id, &request.person_id, "binding-validation");
        let receipt = crate::validation::execute(
            &config.adapters[index],
            &resolved,
            executor,
            sandbox,
            input,
        )
        .await?;
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
            ensure!(!*cancelled.borrow(), Failure(Code::Cancelled));
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
