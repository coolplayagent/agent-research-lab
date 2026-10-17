//! Binding validation and owner reads. Kernel checks may retain private protocol inputs.
use super::*;

pub(super) fn structural(c: &Config, request: &Request) -> Result<()> {
    ensure!(c.require_latest, "enqueue-batch requires latest mode");
    ensure!(request.schema_version == 1, "unsupported batch schema");
    safe_id(&request.batch_id)?;
    ensure!(
        (1..=32).contains(&request.tasks.len()),
        "batch requires 1..32 tasks"
    );
    let members: BTreeSet<_> = request.tasks.iter().map(|task| &task.id).collect();
    let mut earlier = BTreeSet::new();
    for task in &request.tasks {
        safe_id(&task.id)?;
        ensure!(earlier.insert(task.id.clone()), "duplicate task ID");
        if let Some(team) = &task.communication {
            team.validate()?;
        }
        ensure!(!task.prompt.trim().is_empty(), "empty task prompt");
        ensure!(
            task.depth <= 3
                && task
                    .max_attempts
                    .is_none_or(|limit| (1..=3).contains(&limit)),
            "invalid native task limits"
        );
        ensure!(c.models.contains_key(&task.role), "unknown model role");
        registration::repository(c, task)?;
        for tool in &task.required_tools {
            ensure!(c.tools.contains_key(tool), "unknown tool {tool}");
        }
        for dependency in &task.dependencies {
            safe_id(dependency)?;
            ensure!(dependency != &task.id, "task depends on itself");
            ensure!(
                if members.contains(dependency) {
                    earlier.contains(dependency)
                } else {
                    job_path(c, dependency).try_exists()?
                },
                "dependency must exist or precede this task"
            );
            if task.role == "review"
                && let Some(parent) = request.tasks.iter().find(|parent| &parent.id == dependency)
            {
                ensure!(
                    !parent.write
                        || parent.repository != task.repository
                        || job_path(c, dependency).try_exists()?,
                    "review requires an existing committed write candidate"
                );
            }
        }
    }
    Ok(())
}

pub(super) fn new_request_bindings(c: &Config, request: &Request) -> Result<()> {
    for task in &request.tasks {
        if let Some(prior) = read_job(c, &task.id)? {
            requested_matches(task, &prior)?;
            agent_backend::verify_job(c, &prior)?;
            people::validate_frozen_binding(c, &prior)?;
        } else {
            people::validate_batch_persona(c, task)?;
            if task.prompt_version.is_some() {
                PromptBinding::capture(c, task)?;
            }
        }
    }
    Ok(())
}

pub(super) fn read_job(c: &Config, id: &str) -> Result<Option<Job>> {
    let path = job_path(c, id);
    match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
        Ok(_) => Ok(Some(storage::read(&path)?)),
    }
}
pub(super) fn requested_matches(request: &Task, job: &Job) -> Result<()> {
    let mut requested = request.clone();
    if requested.prompt_version.is_none() {
        requested.prompt_version = job.task.prompt_version.clone();
    }
    ensure!(
        digest(&requested)? == digest(&job.task)?,
        "task ID already binds different inputs"
    );
    Ok(())
}

pub(super) fn validate_job(
    c: &Config,
    job: &Job,
    prepared: &registration::Prepared,
    inputs: &inputs::ExecutionInputs,
) -> Result<()> {
    ensure!(
        digest(&registration::Prepared::from_job(c, job)?)? == digest(prepared)?,
        "frozen Job binding changed"
    );
    ensure!(
        job.config_digest == digest(c)?,
        "frozen configuration changed"
    );
    ensure!(
        job.worktree == c.state_dir.join("worktrees").join(&job.task.id),
        "frozen worktree path changed"
    );
    let original = job
        .research_inputs
        .as_ref()
        .context("existing Job lacks latest input binding")?;
    ensure!(
        inputs::identity(original)? == inputs::identity(inputs)?,
        "existing Job input identity changed"
    );
    let repository = inputs
        .repositories
        .get(&job.task.repository)
        .context("missing repository input")?;
    ensure!(
        job.baseline_commit.as_ref().unwrap_or(&job.source_commit) == &repository.upstream.commit,
        "existing Job source baseline changed"
    );
    ensure!(
        job.superpod_commit
            == inputs
                .repositories
                .get("superpod")
                .context("missing SuperPOD input")?
                .upstream
                .commit,
        "existing Job SuperPOD binding changed"
    );
    ensure!(
        storage::digest(rendered_job_prompt(c, job)?.as_bytes()) == job.prompt_digest,
        "existing Job prompt digest changed"
    );
    agent_backend::verify_job(c, job)?;
    people::validate_frozen_binding(c, job)?;
    Ok(())
}

pub(super) fn same_attempt(left: &Job, right: &Job) -> Result<bool> {
    Ok(left.attempt == right.attempt
        && left.run_id == right.run_id
        && frozen_input(left) == frozen_input(right))
}

pub(super) fn repair_eligible(
    c: &Config,
    previous: &Job,
    observation: &mut Observation,
) -> Result<()> {
    let current = read_job(c, &previous.task.id)?.context("Job disappeared before repair")?;
    if !same_attempt(previous, &current)? {
        observation.state = EntryState::Unknown;
        return Err(fail(
            Code::CurrentAttemptChanged,
            "current attempt changed before repair",
        ));
    }
    if current.attempt != 1
        || current.launch.is_some()
        || current.retry_after.is_some()
        || current.last_error.is_some()
    {
        observation.state = EntryState::Unknown;
        return Err(fail(
            Code::OwnerProofUnavailable,
            "current registration no longer permits repair",
        ));
    }
    Ok(())
}

pub(super) fn owner(c: &Config, job: &Job) -> Result<Option<workflow::RegistrationProof>> {
    checkpoint()?;
    let w = workflow(c);
    match fs::symlink_metadata(&w.database) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
        Ok(metadata) => ensure!(
            metadata.is_file() && !metadata.file_type().is_symlink(),
            "owner database path is not a regular file"
        ),
    }
    w.confirm_registration(
        &job.run_id,
        job.task.write,
        frozen_input(job),
        c.task_timeout_seconds * 1000,
    )
}

pub(super) fn observe(c: &Config, job: &Job, observation: &mut Observation) -> Result<()> {
    observation.attempt = Some(job.attempt);
    observation.run_id = Some(job.run_id.clone());
    observation.worktree = Some(job.worktree.clone());
    observation.state = EntryState::Unknown;
    observation.frozen_input_digest = Some(digest(&frozen_input(job))?);
    if !(1..=3).contains(&job.attempt)
        || job.run_id != format!("{}-attempt-{}", job.task.id, job.attempt)
    {
        return Err(fail(
            Code::OwnerProofUnavailable,
            "invalid current attempt binding",
        ));
    }
    let proof = owner(c, job).map_err(|error| {
        let code = if process::deadline_exhausted() {
            Code::DeadlineExceeded
        } else if error
            .downcast_ref::<workflow::ObservationError>()
            .is_some_and(|error| error.code == contracts::WorkflowObservationErrorCode::Mismatch)
        {
            Code::OwnerProofMismatch
        } else {
            Code::OwnerProofUnavailable
        };
        fail(code, format!("{error:#}"))
    })?;
    let current = read_job(c, &job.task.id)?.context("Job disappeared during owner proof")?;
    if !same_attempt(job, &current)? {
        return Err(fail(
            Code::CurrentAttemptChanged,
            "current attempt changed during owner proof",
        ));
    }
    if let Some(proof) = proof {
        observation.state = if job.attempt > 1
            || job.launch.is_some()
            || job.retry_after.is_some()
            || job.last_error.is_some()
        {
            EntryState::AdvancedExistingConfirmed
        } else {
            EntryState::WorkflowConfirmed
        };
        observation.proof = Some(proof);
    } else {
        if current.attempt != 1
            || current.launch.is_some()
            || current.retry_after.is_some()
            || current.last_error.is_some()
        {
            return Err(fail(
                Code::OwnerProofUnavailable,
                "current live/advanced owner run unavailable; repair forbidden",
            ));
        }
        workflow(c).validate_retained_task_start(
            &job.run_id,
            job.task.write,
            frozen_input(job),
            c.task_timeout_seconds * 1000,
        )?;
        observation.state = EntryState::JobRecorded;
    }
    Ok(())
}
