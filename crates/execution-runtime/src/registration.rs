//! Immutable native registration preparation shared by ordinary and batch enqueue.
use super::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Prepared {
    pub task: Task,
    pub persona: Option<task::PersonaSnapshot>,
    pub model: String,
    pub backend: Option<config::Binding>,
    pub repository: PathBuf,
    pub source_commit: String,
    pub baseline_commit: Option<String>,
    pub superpod_commit: String,
    pub config_digest: String,
    pub worktree: PathBuf,
}

pub(super) fn repository(c: &Config, task: &Task) -> Result<PathBuf> {
    if task.repository == "superpod" {
        Ok(c.superpod.clone())
    } else if task.repository == "agent-research-lab" {
        Ok(c.workspace.join("agent-research-lab"))
    } else {
        Ok(c.tools
            .get(&task.repository)
            .context("repository not allowlisted")?
            .repository
            .clone())
    }
}

pub(super) fn prepare(
    c: &Config,
    task: Task,
    inputs: Option<&inputs::ExecutionInputs>,
) -> Result<Prepared> {
    let persona = people::freeze(c, &task)?;
    prepare_frozen(c, task, inputs, persona)
}

pub(super) fn prepare_frozen(
    c: &Config,
    task: Task,
    inputs: Option<&inputs::ExecutionInputs>,
    persona: Option<task::PersonaSnapshot>,
) -> Result<Prepared> {
    prepare_planned(c, task, inputs, persona, &[])
}

pub(super) fn prepare_planned(
    c: &Config,
    task: Task,
    inputs: Option<&inputs::ExecutionInputs>,
    persona: Option<task::PersonaSnapshot>,
    earlier: &[Task],
) -> Result<Prepared> {
    let preference = persona.as_ref().and_then(|p| p.execution.as_ref());
    let backend =
        agent_backend::freeze_selected(c, &task, preference.and_then(|p| p.backend.as_deref()))?;
    let model = preference
        .and_then(|p| p.model.clone())
        .unwrap_or_else(|| c.models[&task.role].clone());
    agent_backend::validate_model(
        c,
        backend.as_ref().map_or("codex", |b| b.id.as_str()),
        &model,
    )?;
    let repository = if task.repository == "superpod" {
        c.superpod.clone()
    } else if task.repository == "agent-research-lab" {
        c.workspace.join("agent-research-lab")
    } else {
        c.tools
            .get(&task.repository)
            .context("repository not allowlisted")?
            .repository
            .clone()
    };
    let mut commit = if let Some(inputs) = inputs {
        inputs
            .repositories
            .get(&task.repository)
            .context("missing latest repository snapshot")?
            .upstream
            .commit
            .clone()
    } else {
        git(&repository, &["rev-parse", "HEAD"])?
    };
    let mut baseline_commit = None;
    if task.role == "review" {
        for id in &task.dependencies {
            if !job_path(c, id).try_exists()?
                && earlier.iter().any(|parent| {
                    &parent.id == id && (!parent.write || parent.repository != task.repository)
                })
            {
                // An earlier native Task already proves this is not a write candidate.
                // No Job or implementation receipt is manufactured for preparation.
                continue;
            }
            let parent: Job = storage::read(&job_path(c, id))?;
            if parent.task.write && parent.task.repository == task.repository {
                ensure!(
                    !persona
                        .as_ref()
                        .zip(parent.persona.as_ref())
                        .is_some_and(|(reviewer, author)| reviewer.id == author.id),
                    "independent review requires a different digital person from the implementation author"
                );
                if let Some(inputs) = inputs {
                    ensure!(
                        parent.source_commit
                            == inputs.repositories[&task.repository].upstream.commit,
                        "candidate baseline is no longer the latest default branch; rerun implementation"
                    );
                }
                let receipt = committed_receipt(c, &workflow(c), &parent)?;
                let candidate = receipt["candidate"]["candidate_commit"]
                    .as_str()
                    .context("implementation has no frozen candidate commit")?;
                ensure!(
                    candidate.len() == 40 && candidate.bytes().all(|b| b.is_ascii_hexdigit()),
                    "invalid candidate commit"
                );
                if baseline_commit.is_some() {
                    ensure!(
                        commit == candidate,
                        "review cannot combine different candidate commits"
                    );
                }
                git(
                    &repository,
                    &["cat-file", "-e", &format!("{candidate}^{{commit}}")],
                )?;
                commit = candidate.into();
                baseline_commit = Some(parent.source_commit.clone());
            }
        }
    }
    let superpod_commit = if let Some(inputs) = inputs {
        inputs.repositories["superpod"].upstream.commit.clone()
    } else {
        git(&c.superpod, &["rev-parse", "HEAD"])?
    };
    Ok(Prepared {
        worktree: c.state_dir.join("worktrees").join(&task.id),
        config_digest: storage::digest(&serde_json::to_vec(c)?),
        task,
        persona,
        model,
        backend,
        repository,
        source_commit: commit,
        baseline_commit,
        superpod_commit,
    })
}

impl Prepared {
    pub fn from_job(c: &Config, job: &Job) -> Result<Self> {
        Ok(Self {
            task: job.task.clone(),
            persona: job.persona.clone(),
            model: job.model.clone(),
            backend: job.backend.clone(),
            repository: repository(c, &job.task)?,
            source_commit: job.source_commit.clone(),
            baseline_commit: job.baseline_commit.clone(),
            superpod_commit: job.superpod_commit.clone(),
            config_digest: job.config_digest.clone(),
            worktree: job.worktree.clone(),
        })
    }
    pub fn job(&self, c: &Config, inputs: Option<inputs::ExecutionInputs>) -> Result<Job> {
        let prompt_digest = storage::digest(
            format!(
                "{}{}",
                rendered_experiment_prompt(c, &self.task, inputs.as_ref())?,
                people::prompt(self.persona.as_ref())?
            )
            .as_bytes(),
        );
        Ok(Job {
            persona: self.persona.clone(),
            model: self.model.clone(),
            backend: self.backend.clone(),
            source_commit: self.source_commit.clone(),
            baseline_commit: self.baseline_commit.clone(),
            research_inputs: inputs,
            superpod_commit: self.superpod_commit.clone(),
            prompt_digest,
            config_digest: self.config_digest.clone(),
            worktree: self.worktree.clone(),
            run_id: format!("{}-attempt-1", self.task.id),
            attempt: 1,
            last_error: None,
            launch: None,
            retry_after: None,
            task: self.task.clone(),
        })
    }
    pub fn create_worktree(&self) -> Result<()> {
        fs::create_dir_all(self.worktree.parent().context("worktree parent missing")?)?;
        if self.worktree.exists() {
            self.validate_existing_worktree()?;
        } else {
            git(
                &self.repository,
                &[
                    "worktree",
                    "add",
                    "--detach",
                    self.worktree.to_str().context("UTF8 path")?,
                    &self.source_commit,
                ],
            )?;
        }
        Ok(())
    }
    pub fn validate_existing_worktree(&self) -> Result<()> {
        if self.worktree.try_exists()? {
            ensure!(
                git(&self.worktree, &["rev-parse", "HEAD"])? == self.source_commit,
                "interrupted worktree has different base"
            );
        }
        Ok(())
    }
}

/// The caller owns enqueue.lock. Each callback precedes a native effect.
pub(super) fn register(
    c: &Config,
    prepared: &Prepared,
    inputs: Option<inputs::ExecutionInputs>,
    mut before: impl FnMut(contracts::BatchRegistrationPhase, Option<&Job>) -> Result<()>,
) -> Result<Job> {
    use contracts::BatchRegistrationPhase as Phase;
    before(Phase::RegisterJob, None)?;
    prepared.create_worktree()?;
    let job = prepared.job(c, inputs)?;
    before(Phase::RegisterJob, None)?;
    storage::write(&job_path(c, &job.task.id), &job)?;
    before(Phase::BindPersona, Some(&job))?;
    people::bind(c, &job)?;
    before(Phase::ConfirmWorkflow, Some(&job))?;
    start(c, &job, || before(Phase::ConfirmWorkflow, Some(&job)))?;
    Ok(job)
}

pub(super) fn start(c: &Config, job: &Job, mut before: impl FnMut() -> Result<()>) -> Result<()> {
    before()?;
    let owner = workflow(c);
    owner.initialize()?;
    before()?;
    ensure_started_in_store(c, &owner, job)
}
