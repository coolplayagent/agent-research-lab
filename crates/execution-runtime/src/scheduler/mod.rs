//! Single-controller orchestration of admission, workers and recovery.
mod cohort;
mod eligibility;
mod workers;

use crate::admission;
use crate::host_budget::{
    RunWindow, idle_host, load_ledger, now, prepare_host, prepare_prerequisite,
};
use crate::job_store::{ensure_started_in_store, jobs, workflow};
use crate::launch::{launch, launch_guarded};
use crate::post_success::{drain_followups_scoped, validate_state};
use crate::recovery::{kill_retained_workers, reconcile_orphans};
use crate::scope::{TaskScope, scoped_run_status};
use crate::seeding::seed_inputs;
use crate::watch;
use anyhow::{Context, Result, ensure};
use cohort::{Preparation, PreparedCohort, Prerequisites};
use config::Config;
use contracts::ControllerPhase;
use eligibility::admission_ready;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::Path;
use std::time::Duration;
use workers::Workers;

/// One controller admits work; workflow-cli remains the authority for each durable job.
pub fn run(c: &Config, continuous: bool, max_seconds: u64) -> Result<Value> {
    run_seeded(c, continuous, max_seconds, false)
}

pub fn run_seeded(
    c: &Config,
    continuous: bool,
    max_seconds: u64,
    seed_requested: bool,
) -> Result<Value> {
    run_scoped(c, continuous, max_seconds, seed_requested, None)
}

pub fn run_scoped(
    c: &Config,
    continuous: bool,
    max_seconds: u64,
    seed_requested: bool,
    task_ids_file: Option<&Path>,
) -> Result<Value> {
    run_scoped_impl(
        c,
        continuous,
        max_seconds,
        seed_requested,
        task_ids_file,
        None,
    )
}

pub fn run_scoped_with_admission(
    c: &Config,
    max_seconds: u64,
    task_ids_file: &Path,
    hook: &mut dyn admission::ScopedAdmissionHook,
    capture_observer: std::sync::Arc<dyn process::CaptureObserver>,
) -> Result<Value> {
    let _observer = process::observer_scope(capture_observer);
    run_scoped_impl(
        c,
        false,
        max_seconds,
        false,
        Some(task_ids_file),
        Some(hook),
    )
}

/// The runtime owns the one finalization, after its lock, watch and worker guards drop.
pub struct ObservedRun {
    pub result: Result<Value>,
    pub filesystem: Result<contracts::NativeFilesystemReport>,
}

/// The caller prepares both observer domains before entering this function.
/// A session is live source-owned authority, not a deserializable success flag.
pub fn run_scoped_with_observations(
    c: &Config,
    max_seconds: u64,
    task_ids_file: &Path,
    hook: &mut dyn admission::ScopedAdmissionHook,
    session: native_effects::NativeObservationSession,
) -> ObservedRun {
    let result = (|| {
        let _effects = native_effects::observation_scope(&session)?;
        run_scoped_impl(
            c,
            false,
            max_seconds,
            false,
            Some(task_ids_file),
            Some(hook),
        )
    })();
    let filesystem = session.finish();
    ObservedRun { result, filesystem }
}

pub(super) fn run_scoped_impl(
    c: &Config,
    continuous: bool,
    max_seconds: u64,
    seed_requested: bool,
    task_ids_file: Option<&Path>,
    mut admission_hook: Option<&mut dyn admission::ScopedAdmissionHook>,
) -> Result<Value> {
    c.validate()?;
    validate_state(c)?;
    ensure!(
        task_ids_file.is_none() || (!continuous && !seed_requested && max_seconds > 0),
        "scoped runs require a positive max-seconds, no continuous mode and no seed"
    );
    let start = now();
    let window = RunWindow {
        started: start,
        max_seconds,
    };
    let _lock = storage::lock(&c.state_dir.join("controller.lock"))?;
    if let Some(hook) = admission_hook.as_deref_mut() {
        hook.register_cleanup_domain(
            c,
            task_ids_file.context("guarded admission requires an explicit task scope")?,
            max_seconds,
        )?;
    }
    let mut observer = watch::Controller::start(c)?;
    kill_retained_workers(c)?;
    let scope = task_ids_file
        .map(|path| TaskScope::load(c, path))
        .transpose()?;
    let mut deferred_followups = BTreeSet::new();
    let w = workflow(c);
    let ledger_path = c.state_dir.join("budget.json");
    let mut ledger = load_ledger(c)?;
    while window.remaining(c, &ledger) == 0 {
        if !continuous || max_seconds > 0 || c.state_dir.join("paused").exists() {
            return scoped_run_status(c, scope.as_ref(), &deferred_followups);
        }
        std::thread::sleep(Duration::from_secs(1));
        ledger.budget.tick(now(), false)?;
    }
    if idle_host(c, &mut ledger, window, || {
        w.version()?;
        w.initialize()?;
        for job in jobs(c)? {
            ensure_started_in_store(c, &w, &job)?;
            observer.record(ControllerPhase::Recovering, &[])?;
        }
        reconcile_orphans(c, &w)?;
        if seed_requested {
            observer.record(ControllerPhase::Refreshing, &[])?;
            if let Err(error) = watch::seed(c, || seed_inputs(c))
                && !continuous
            {
                return Err(error);
            }
        }
        Ok(())
    })?
    .is_none()
    {
        return scoped_run_status(c, scope.as_ref(), &deferred_followups);
    }
    let exploration_path = c.state_dir.join("evolution").join("exploration.json");
    if exploration_path.exists() {
        let exploration: evolution::ExplorationBudget = storage::read(&exploration_path)?;
        exploration.validate()?;
        ledger.exploration_percent = exploration.share_percent();
    }
    let mut last = now();
    let mut next_seed_check = watch::next_seed(c);
    let mut next_host_tick = 0_i64;
    let mut prerequisites = Prerequisites::default();
    let mut workers = Workers::default();
    let mut proof_prepared = false;
    let mut guarded_admission_failed = false;
    loop {
        let time = now();
        observer.record(
            if workers.is_empty() {
                ControllerPhase::Idle
            } else {
                ControllerPhase::Running
            },
            &workers.task_ids(),
        )?;
        let elapsed = (time - last).max(0) as u64;
        if let Some(exploratory) = workers.exploratory() {
            if exploratory {
                ledger.exploration_seconds += elapsed;
            } else {
                ledger.main_seconds += elapsed;
            }
        }
        ledger.budget.tick(time, !workers.is_empty())?;
        last = time;
        storage::write(&ledger_path, &ledger)?;
        let stopping = (max_seconds > 0 && time - start >= max_seconds.min(i64::MAX as u64) as i64)
            || c.state_dir.join("paused").exists()
            || ledger.budget.remaining(time, c.daily_seconds) == 0
            || guarded_admission_failed
            || admission_hook
                .as_deref()
                .is_some_and(|hook| hook.should_stop());
        guarded_admission_failed |= workers.poll(c, &w, stopping, time, &mut admission_hook)?;
        if stopping {
            ledger.budget.tick(now(), false)?;
            storage::write(&ledger_path, &ledger)?;
            if !continuous || max_seconds > 0 || c.state_dir.join("paused").exists() {
                break;
            }
            std::thread::sleep(Duration::from_secs(1));
            continue;
        }
        let mut host_progress = false;
        if workers.is_empty() && jobs(c)?.iter().all(|job| job.launch.is_none()) {
            match idle_host(c, &mut ledger, window, || {
                drain_followups_scoped(c, &w, scope.as_ref(), &mut deferred_followups)
            }) {
                Ok(Some(progress)) => host_progress |= progress,
                Ok(None) => continue,
                Err(error) => storage::write(
                    &c.state_dir.join("completion-error.json"),
                    &json!({"at":now(),"error":format!("{error:#}")}),
                )?,
            }
            last = now();
        }
        let outbox = c.state_dir.join("outbox");
        if scope.is_none()
            && workers.is_empty()
            && outbox.exists()
            && time >= next_host_tick
            && jobs(c)?.iter().all(|job| job.launch.is_none())
        {
            match idle_host(c, &mut ledger, window, || automation::tick(&outbox)) {
                Ok(Some(entry)) => {
                    host_progress |= entry.is_some();
                    next_host_tick = now() + 1;
                }
                Ok(None) => continue,
                Err(error) => {
                    storage::write(
                        &c.state_dir.join("automation-error.json"),
                        &json!({"at":now(),"error":format!("{error:#}")}),
                    )?;
                    next_host_tick = now() + 30;
                }
            }
            last = now();
        }
        if window.remaining(c, &ledger) == 0 {
            continue;
        }
        let total = ledger.main_seconds + ledger.exploration_seconds;
        let prefer_exploration = total > 0
            && ledger.exploration_seconds * 100 < total * ledger.exploration_percent as u64;
        let track = workers.exploratory().unwrap_or(prefer_exploration);
        let mut pending = jobs(c)?;
        let may_admit = workers.is_empty() && pending.iter().all(|job| job.launch.is_none());
        pending.retain(|job| {
            scope
                .as_ref()
                .is_none_or(|scope| scope.ids.contains(&job.task.id))
        });
        pending.sort_by_key(|j| j.task.exploratory != track);
        let mut admitted = false;
        let admission_deadline =
            process::deadline_scope(Duration::from_secs(window.remaining(c, &ledger)));
        let PreparedCohort {
            jobs: prepared,
            retry_waiting,
            mut deferred_admission,
        } = prerequisites.prepare(
            pending,
            may_admit,
            Preparation {
                config: c,
                workflow: &w,
                ledger: &mut ledger,
                window,
                time,
                last: &mut last,
                guarded: admission_hook.is_some(),
            },
        )?;
        if !prepared.is_empty()
            && !proof_prepared
            && let Some(hook) = admission_hook.as_deref_mut()
        {
            if let Err(error) = hook.prepare(c, &prepared) {
                hook.revoke(&format!("guarded proof failed: {error:#}"));
                return Err(error);
            }
            proof_prepared = true;
        }
        for mut job in prepared {
            if guarded_admission_failed
                || admission_hook
                    .as_deref()
                    .is_some_and(|hook| hook.should_stop())
            {
                break;
            }
            if workers.len() >= c.max_agents {
                break;
            }
            if !workers.may_admit(&job) {
                deferred_admission = true;
                continue;
            }
            let eligible =
                prepare_prerequisite(c, &mut ledger, window, !workers.is_empty(), || {
                    admission_ready(c, &w, &job)
                })?;
            if workers.is_empty() {
                last = now();
            }
            match eligible {
                Some(true) => {}
                Some(false) => continue,
                None => break,
            }
            ledger.budget.tick(now(), !workers.is_empty())?;
            let bounded_remaining = window.remaining(c, &ledger);
            if bounded_remaining == 0 {
                break;
            }
            let guarded = admission_hook.is_some();
            let Ok(_activation_admission) =
                storage::lock(&c.state_dir.join("private/software-admission.lock"))
            else {
                deferred_admission = true;
                continue;
            };
            if c.state_dir.join("private/activation-drain").exists() {
                deferred_admission = true;
                continue;
            }
            let mut job_writes = admission::JobWrites::new(&job, guarded);
            let launched = prepare_host(c, &mut ledger, window, !workers.is_empty(), || {
                if guarded {
                    launch_guarded(
                        c,
                        &w,
                        &mut job,
                        bounded_remaining,
                        &mut admission_hook,
                        &mut job_writes,
                    )
                } else {
                    launch(c, &w, &mut job, bounded_remaining)
                }
            });
            if workers.is_empty() {
                last = now();
            }
            match launched {
                Ok(Some((process, heartbeat))) => {
                    workers.push(job, job_writes, process, heartbeat);
                    admitted = true;
                }
                Ok(None) => break,
                Err(error) => {
                    let _cleanup = process::cleanup_scope();
                    if let Some(hook) = admission_hook.as_deref_mut() {
                        hook.revoke(&format!("guarded launch failed: {error:#}"));
                        guarded_admission_failed = true;
                    }
                    admission::launch_failed(c, &w, &mut job, &mut job_writes, &error, guarded)?;
                }
            }
        }
        drop(admission_deadline);
        if !workers.is_empty() && ledger.budget.active_since.is_none() {
            ledger.budget.tick(now(), true)?;
            storage::write(&ledger_path, &ledger)?;
        }
        if workers.is_empty() && guarded_admission_failed {
            break;
        }
        if workers.is_empty() && !admitted {
            ledger.budget.tick(now(), false)?;
            storage::write(&ledger_path, &ledger)?;
            if !continuous && !retry_waiting && !host_progress && !deferred_admission {
                break;
            }
            if continuous && now() >= next_seed_check {
                observer.record(ControllerPhase::Refreshing, &[])?;
                if let Err(error) =
                    idle_host(c, &mut ledger, window, || watch::seed(c, || seed_inputs(c)))
                {
                    storage::write(
                        &c.state_dir.join("seed-error.json"),
                        &json!({"at":now(),"error":format!("{error:#}")}),
                    )?;
                }
                last = now();
                next_seed_check = watch::next_seed(c);
            }
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    scoped_run_status(c, scope.as_ref(), &deferred_followups)
}
