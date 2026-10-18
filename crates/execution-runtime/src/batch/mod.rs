//! Bounded registration of native Tasks, with one fresh input acquisition.
//! Registration does not launch workers or provide atomic cohort admission.
use crate::{
    job_store::{frozen_input, workflow},
    people, registration,
};
use anyhow::{Context, Result, ensure};
use config::Config;
use contracts::{
    BatchEntryRegistrationState as EntryState, BatchRegistrationErrorCode as Code,
    BatchRegistrationPhase as Phase, BatchRegistrationState as State,
};
use plan::{Entry, Plan, PromptBinding, digest};
use std::{path::Path, time::Duration};
use validation::{
    new_request_bindings, observe, owner, read_job, repair_eligible, requested_matches, structural,
    validate_job,
};
mod plan;
mod receipt;
mod validation;
pub use plan::Request;
pub use receipt::{Failure, Observation, Receipt};

// Test observers coordinate real filesystem/owner faults, never replace effects.
#[cfg(test)]
type Observer = Box<dyn FnMut(Phase, &str) -> Result<()>>;
#[cfg(test)]
thread_local! { static OBSERVER: std::cell::RefCell<Option<Observer>> = const { std::cell::RefCell::new(None) }; }
fn notify(phase: Phase, task: &str) -> Result<()> {
    #[cfg(test)]
    return OBSERVER.with(|observer| {
        if let Some(observer) = observer.borrow_mut().as_mut() {
            observer(phase, task)
        } else {
            Ok(())
        }
    });
    #[cfg(not(test))]
    {
        let _ = (phase, task);
        Ok(())
    }
}

fn fail(code: Code, message: impl std::fmt::Display) -> anyhow::Error {
    Failure {
        code,
        message: message.to_string(),
        registration: None,
    }
    .into()
}
fn tagged<T>(code: Code, result: Result<T>) -> Result<T> {
    result.map_err(|error| {
        fail(
            if process::deadline_exhausted() {
                Code::DeadlineExceeded
            } else {
                code
            },
            format!("{error:#}"),
        )
    })
}
fn checkpoint() -> Result<()> {
    if process::deadline_exhausted() {
        return Err(fail(
            Code::DeadlineExceeded,
            "batch registration deadline exceeded",
        ));
    }
    Ok(())
}

/// Read a bounded request and register it through the production native path.
pub fn enqueue(c: &Config, path: &Path, max_seconds: u64) -> Result<Receipt> {
    if !(1..=600).contains(&max_seconds) {
        return Err(fail(Code::InvalidRequest, "max-seconds must be 1..600"));
    }
    let _deadline = process::deadline_scope(Duration::from_secs(max_seconds));
    let (request, raw_digest) = tagged(Code::InvalidRequest, plan::read_request(path))?;
    enqueue_collected(c, request, raw_digest, max_seconds, || {
        inputs::collect(c)?.context("batch requires latest inputs")
    })
}

fn build_plan(
    c: &Config,
    request: &Request,
    raw: &str,
    fresh: inputs::ExecutionInputs,
) -> Result<Plan> {
    // Validate all already-frozen identities before any new persona memory read.
    for requested in &request.tasks {
        if let Some(job) = read_job(c, &requested.id)? {
            requested_matches(requested, &job)?;
            validate_job(c, &job, &registration::Prepared::from_job(c, &job)?, &fresh)?;
            PromptBinding::capture(c, &job.task)?;
        }
    }
    let mut entries = Vec::new();
    for (index, requested) in request.tasks.iter().enumerate() {
        checkpoint()?;
        let prepared = if let Some(job) = read_job(c, &requested.id)? {
            requested_matches(requested, &job)?;
            let prepared = registration::Prepared::from_job(c, &job)?;
            validate_job(c, &job, &prepared, &fresh)?;
            prepared
        } else {
            let mut task = requested.clone();
            if task.prompt_version.is_none() {
                task.prompt_version = evolution_cli::resolve_selection(&c.state_dir, &task.role)?;
            }
            let persona = people::freeze_for_batch(c, &task)?;
            registration::prepare_planned(c, task, Some(&fresh), persona, &request.tasks[..index])?
        };
        let prompt = PromptBinding::capture(c, &prepared.task)?;
        entries.push(Entry { prepared, prompt });
    }
    Ok(Plan {
        schema_version: 1,
        request: request.clone(),
        request_digest: digest(request)?,
        original_request_sha256: raw.into(),
        config_digest: digest(c)?,
        input_identity: inputs::identity(&fresh)?,
        inputs: fresh,
        entries,
    })
}

fn enqueue_collected(
    c: &Config,
    request: Request,
    raw: String,
    max_seconds: u64,
    collect: impl FnOnce() -> Result<inputs::ExecutionInputs>,
) -> Result<Receipt> {
    tagged(
        Code::InvalidRequest,
        (|| {
            ensure!(
                (1..=600).contains(&max_seconds),
                "max-seconds must be 1..600"
            );
            Ok(())
        })(),
    )?;
    let _deadline = process::deadline_scope(Duration::from_secs(max_seconds));
    tagged(Code::InvalidRequest, structural(c, &request))?;
    let root = c
        .state_dir
        .join("private/enqueue-batches")
        .join(&request.batch_id);
    let _batch_lock = tagged(
        Code::RegistrationFailure,
        storage::lock(&root.join("batch.lock")),
    )?;
    let plan_path = root.join("plan.json");
    let retained: Option<Plan> = if plan_path.try_exists()? {
        Some(storage::read(&plan_path)?)
    } else {
        None
    };
    if let Some(plan) = &retained {
        if plan.request_digest != digest(&request)? || digest(&plan.request)? != plan.request_digest
        {
            return Err(fail(
                Code::RequestConflict,
                "batch ID already binds a different request",
            ));
        }
    } else {
        tagged(
            Code::FrozenBindingConflict,
            new_request_bindings(c, &request),
        )?;
    }
    let mut receipt = Receipt {
        schema_version: 1,
        batch_id: request.batch_id.clone(),
        request_digest: digest(&request)?,
        observed_request_sha256: raw.clone(),
        plan_digest: None,
        config_digest: digest(c)?,
        input_identity: None,
        collected_inputs_digest: None,
        started_at_ms: workflow::now_ms()?,
        finished_at_ms: None,
        max_seconds,
        state: State::Incomplete,
        phase: Phase::Preflight,
        error_code: None,
        entries: request
            .tasks
            .iter()
            .map(|task| Observation {
                task_id: task.id.clone(),
                state: EntryState::Pending,
                attempt: None,
                run_id: None,
                worktree: None,
                proof: None,
                frozen_input_digest: None,
            })
            .collect(),
        receipt_path: root.join("receipt.json"),
        receipt_persisted: false,
    };
    let outcome = register_batch(
        c,
        &request,
        &raw,
        retained.as_ref(),
        &plan_path,
        &mut receipt,
        collect,
    );
    receipt.finished_at_ms = Some(workflow::now_ms()?);
    let error = outcome.err();
    if let Some(error) = &error {
        receipt.error_code = Some(
            error
                .downcast_ref::<Failure>()
                .map_or(Code::RegistrationFailure, |e| e.code),
        );
        if receipt
            .entries
            .iter()
            .all(|entry| entry.state == EntryState::Pending)
            && matches!(
                receipt.error_code,
                Some(
                    Code::InvalidRequest
                        | Code::RequestConflict
                        | Code::FrozenBindingConflict
                        | Code::FreshnessDrift
                )
            )
        {
            receipt.state = State::Rejected;
        }
    } else {
        receipt.state = State::Complete;
    }
    if let Err(error) = receipt.save() {
        receipt.state = State::Incomplete;
        receipt.error_code = Some(Code::ReceiptPersistenceFailure);
        return Err(Failure {
            code: Code::ReceiptPersistenceFailure,
            message: format!("receipt persistence failed: {error:#}"),
            registration: Some(Box::new(receipt)),
        }
        .into());
    }
    if let Some(error) = error {
        return Err(Failure {
            code: receipt.error_code.unwrap_or(Code::RegistrationFailure),
            message: format!("{error:#}"),
            registration: Some(Box::new(receipt)),
        }
        .into());
    }
    Ok(receipt)
}

fn register_batch(
    c: &Config,
    request: &Request,
    raw: &str,
    retained: Option<&Plan>,
    path: &Path,
    receipt: &mut Receipt,
    collect: impl FnOnce() -> Result<inputs::ExecutionInputs>,
) -> Result<()> {
    receipt.phase = Phase::Collect;
    checkpoint()?;
    let fresh = tagged(Code::FreshnessFailure, collect())?;
    checkpoint()?;
    receipt.input_identity = Some(inputs::identity(&fresh)?);
    receipt.collected_inputs_digest = Some(digest(&fresh)?);
    receipt.phase = Phase::Prepare;
    let plan = if let Some(plan) = retained {
        tagged(
            Code::FreshnessDrift,
            (|| {
                ensure!(
                    plan.schema_version == 1 && plan.config_digest == digest(c)?,
                    "frozen batch configuration changed"
                );
                ensure!(
                    plan.input_identity == inputs::identity(&plan.inputs)?
                        && plan.input_identity == inputs::identity(&fresh)?,
                    "fresh full input identity changed"
                );
                ensure!(
                    plan.entries.len() == request.tasks.len(),
                    "invalid retained entry count"
                );
                Ok(())
            })(),
        )?;
        plan.clone()
    } else {
        tagged(
            Code::FrozenBindingConflict,
            build_plan(c, request, raw, fresh.clone()),
        )?
    };
    receipt.plan_digest = Some(digest(&plan)?);
    ensure!(
        serde_json::to_vec_pretty(&plan)?.len() <= plan::PLAN_LIMIT,
        "retained plan exceeds 16 MiB"
    );
    checkpoint()?;
    notify(Phase::Prepare, "")?;
    let _enqueue_lock = tagged(
        Code::RegistrationFailure,
        storage::lock(&c.state_dir.join("enqueue.lock")),
    )?;
    notify(Phase::Preflight, "")?;
    // Revalidate every member before any native registration or repair effect.
    for (index, entry) in plan.entries.iter().enumerate() {
        checkpoint()?;
        tagged(
            Code::FrozenBindingConflict,
            entry.prompt.verify(c, &entry.prepared.task),
        )?;
        ensure!(
            entry.prepared.task.id == request.tasks[index].id,
            "retained plan order changed"
        );
        let mut requested = request.tasks[index].clone();
        if requested.prompt_version.is_none() {
            requested.prompt_version = entry.prepared.task.prompt_version.clone();
        }
        ensure!(
            digest(&requested)? == digest(&entry.prepared.task)?,
            "retained prepared Task differs from requested Task"
        );
        if let Some(job) = read_job(c, &entry.prepared.task.id)? {
            tagged(
                Code::FrozenBindingConflict,
                validate_job(c, &job, &entry.prepared, &fresh),
            )?;
            observe(c, &job, &mut receipt.entries[index])?;
        } else {
            let job = entry.prepared.job(c, Some(fresh.clone()))?;
            tagged(
                Code::FrozenBindingConflict,
                validate_job(c, &job, &entry.prepared, &fresh),
            )?;
            tagged(
                Code::FrozenBindingConflict,
                entry.prepared.validate_existing_worktree(),
            )?;
            tagged(
                Code::FrozenBindingConflict,
                workflow(c).validate_retained_task_start(
                    &job.run_id,
                    job.task.write,
                    frozen_input(&job),
                    c.task_timeout_seconds * 1000,
                ),
            )?;
            // A run without its Job cannot be silently adopted or overwritten.
            ensure!(
                owner(c, &job)?.is_none(),
                "owner run exists without matching Job"
            );
        }
    }
    checkpoint()?;
    receipt.phase = Phase::PersistPlan;
    notify(receipt.phase, "")?;
    if retained.is_none() {
        plan::publish(path, &plan)?;
    }
    receipt.save()?;
    for (index, entry) in plan.entries.iter().enumerate() {
        checkpoint()?;
        if matches!(
            receipt.entries[index].state,
            EntryState::WorkflowConfirmed | EntryState::AdvancedExistingConfirmed
        ) {
            let confirmed = receipt.entries[index].state;
            // The proof is historical until the current Job passes settlement.
            receipt.entries[index].state = EntryState::Unknown;
            let job = read_job(c, &entry.prepared.task.id)?.context("confirmed Job disappeared")?;
            if receipt.entries[index].attempt != Some(job.attempt)
                || receipt.entries[index].run_id.as_ref() != Some(&job.run_id)
            {
                return Err(fail(
                    Code::CurrentAttemptChanged,
                    "current attempt changed before confirmation settlement",
                ));
            }
            if receipt.entries[index].frozen_input_digest.as_ref()
                != Some(&digest(&frozen_input(&job))?)
            {
                return Err(fail(
                    Code::FrozenBindingConflict,
                    "immutable Job changed before confirmation settlement",
                ));
            }
            validate_job(c, &job, &entry.prepared, &fresh)?;
            receipt.entries[index].state = confirmed;
            continue;
        }
        if let Some(job) = read_job(c, &entry.prepared.task.id)? {
            validate_job(c, &job, &entry.prepared, &fresh)?;
            // Controller progress is outside enqueue.lock; never overwrite its Job.
            observe(c, &job, &mut receipt.entries[index])?;
            if receipt.entries[index].state == EntryState::JobRecorded {
                checkpoint()?;
                receipt.phase = Phase::BindPersona;
                notify(receipt.phase, &job.task.id)?;
                repair_eligible(c, &job, &mut receipt.entries[index])?;
                people::bind(c, &job)?;
                checkpoint()?;
                receipt.phase = Phase::ConfirmWorkflow;
                notify(receipt.phase, &job.task.id)?;
                receipt.entries[index].state = EntryState::Unknown;
                registration::start(c, &job, || {
                    checkpoint()?;
                    repair_eligible(c, &job, &mut receipt.entries[index])
                })?;
            }
            observe(c, &job, &mut receipt.entries[index])?;
        } else {
            let result =
                registration::register(c, &entry.prepared, Some(fresh.clone()), |phase, job| {
                    receipt.phase = phase;
                    receipt.entries[index].worktree = entry
                        .prepared
                        .worktree
                        .exists()
                        .then(|| entry.prepared.worktree.clone());
                    if let Some(job) = job {
                        receipt.entries[index].attempt = Some(job.attempt);
                        receipt.entries[index].run_id = Some(job.run_id.clone());
                        receipt.entries[index].state = if phase == Phase::ConfirmWorkflow {
                            EntryState::Unknown
                        } else {
                            EntryState::JobRecorded
                        };
                    }
                    checkpoint()?;
                    notify(phase, &entry.prepared.task.id)?;
                    if let Some(job) = job {
                        repair_eligible(c, job, &mut receipt.entries[index])?;
                    }
                    receipt.save()
                });
            match result {
                Ok(job) => observe(c, &job, &mut receipt.entries[index])?,
                Err(error) => {
                    // A typed absent run can distinguish a rejected start from a lost reply.
                    if receipt.phase == Phase::ConfirmWorkflow
                        && let Some(job) = read_job(c, &entry.prepared.task.id)?
                        && job.attempt == 1
                        && job.launch.is_none()
                        && job.retry_after.is_none()
                        && job.last_error.is_none()
                        && matches!(owner(c, &job), Ok(None))
                    {
                        receipt.entries[index].state = EntryState::JobRecorded;
                    }
                    return Err(error);
                }
            }
        }
        ensure!(
            matches!(
                receipt.entries[index].state,
                EntryState::WorkflowConfirmed | EntryState::AdvancedExistingConfirmed
            ),
            "owner registration remains unconfirmed"
        );
        receipt.phase = Phase::PersistReceipt;
        notify(receipt.phase, &entry.prepared.task.id)?;
        receipt.save()?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
