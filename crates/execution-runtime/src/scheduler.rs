//! Single-controller orchestration of admission, workers and recovery.
use crate::admission;
use crate::completion::{ExitObservation, settle_with_heartbeat};
use crate::host_budget::{
    RunWindow, idle_host, load_ledger, now, prepare_host, prepare_prerequisite,
};
use crate::job_store::{ensure_started_in_store, job_path, jobs, workflow};
use crate::jobs::{defer_read_retry, prepare_retry};
use crate::launch::{launch, launch_guarded};
use crate::post_success::{drain_followups_scoped, queue_post_success, validate_state};
use crate::recovery::{kill_retained_workers, reconcile_orphans};
use crate::scope::{TaskScope, scoped_run_status};
use crate::seeding::seed_inputs;
use crate::team_channel::revoke_communication;
use crate::watch;
use anyhow::{Context, Result, ensure};
use config::Config;
use contracts::{ControllerPhase, ExecutionState, WorkflowState};
use native_effects::fs;
use process::Process;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::Duration;
use task::Job;
use workflow::Workflow;

struct Active {
    job: Job,
    job_writes: admission::JobWrites,
    process: Process,
    heartbeat: lease_heartbeat::LeaseHeartbeat,
}

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
    let mut next_communication_poll = 0_i64;
    let mut desktop_prepared = false;
    let mut next_desktop_prepare = 0_i64;
    let mut next_build_prepare = BTreeMap::<String, i64>::new();
    let mut active: Vec<Active> = vec![];
    let mut proof_prepared = false;
    let mut guarded_admission_failed = false;
    loop {
        let time = now();
        observer.record(
            if active.is_empty() {
                ControllerPhase::Idle
            } else {
                ControllerPhase::Running
            },
            &active
                .iter()
                .map(|a| a.job.task.id.clone())
                .collect::<Vec<_>>(),
        )?;
        let elapsed = (time - last).max(0) as u64;
        if !active.is_empty() {
            if active[0].job.task.exploratory {
                ledger.exploration_seconds += elapsed;
            } else {
                ledger.main_seconds += elapsed;
            }
        }
        ledger.budget.tick(time, !active.is_empty())?;
        last = time;
        storage::write(&ledger_path, &ledger)?;
        let stopping = (max_seconds > 0 && time - start >= max_seconds.min(i64::MAX as u64) as i64)
            || c.state_dir.join("paused").exists()
            || ledger.budget.remaining(time, c.daily_seconds) == 0
            || guarded_admission_failed
            || admission_hook
                .as_deref()
                .is_some_and(|hook| hook.should_stop());
        if !stopping && !active.is_empty() && time >= next_communication_poll {
            let members = active
                .iter()
                .filter_map(|a| {
                    a.job
                        .launch
                        .as_ref()?
                        .communication
                        .as_ref()
                        .map(|v| v.member.clone())
                })
                .collect::<Vec<_>>();
            if !members.is_empty() {
                let report =
                    match multi_agent::poll_active(&c.state_dir, &members, time.max(0) as u64) {
                        Ok(report) => report,
                        Err(error) => json!({"error":error.to_string()}),
                    };
                let _ = storage::write(
                    &c.state_dir.join("communication/poll-status.json"),
                    &json!({"at":time,"result":report}),
                );
            }
            next_communication_poll = time + 2;
        }
        let mut i = 0;
        while i < active.len() {
            let mut polled = if stopping {
                active[i].process.cancel().map(|_| None)
            } else {
                active[i].process.poll()
            };
            if let Some(error) = active[i].heartbeat.failure() {
                let _ = active[i].process.cancel();
                polled = Err(anyhow::anyhow!("lease heartbeat failed: {error}"));
            }
            if stopping || !matches!(polled, Ok(None)) {
                let _cleanup = process::cleanup_scope();
                let mut a = active.remove(i);
                a.heartbeat.disarm_worker();
                if let Err(drift) = a.job_writes.confirm(c) {
                    if let Some(hook) = admission_hook.as_deref_mut() {
                        hook.revoke(&format!("guarded active Job changed: {drift:#}"));
                    }
                    let lease = a.heartbeat.stop()?;
                    let _ = w.release(&lease);
                    return Err(drift);
                }
                let observation = ExitObservation {
                    success: matches!(&polled,Ok(Some(status)) if status.success()) && !stopping,
                    code: match &polled {
                        Ok(Some(status)) => status.code(),
                        _ => None,
                    },
                    reason: if stopping {
                        Some("controller budget/pause stop".into())
                    } else {
                        polled.as_ref().err().map(ToString::to_string)
                    },
                };
                a.job
                    .launch
                    .as_mut()
                    .context("active job lacks launch")?
                    .lease = a.heartbeat.current()?;
                let mut launch = a
                    .job
                    .launch
                    .as_ref()
                    .context("active job lacks launch")?
                    .clone();
                storage::write(
                    &w.work_dir
                        .join("host-exits")
                        .join(format!("{}.json", a.job.run_id)),
                    &observation,
                )?;
                match settle_with_heartbeat(&w, &a.job, &observation, Some(&a.heartbeat)) {
                    Ok(settled_success) => {
                        if !settled_success || launch.communication_gap.is_some() {
                            revoke_communication(c, &a.job, &launch);
                        }
                        a.job.launch = None;
                        if settled_success {
                            a.job.last_error = None;
                            a.job.retry_after = None;
                            queue_post_success(c, &w, &a.job)?;
                        } else {
                            let reason = observation
                                .reason
                                .clone()
                                .unwrap_or_else(|| format!("worker exited {:?}", observation.code));
                            if let Some(hook) = admission_hook.as_deref_mut() {
                                hook.revoke(&format!("guarded worker failed: {reason}"));
                                guarded_admission_failed = true;
                                a.job.last_error = Some(reason);
                                a.job.retry_after = None;
                            } else {
                                defer_read_retry(&mut a.job, reason);
                            }
                        }
                    }
                    Err(error) => {
                        revoke_communication(c, &a.job, &launch);
                        // Keep the claim for startup reconciliation after ambiguous settlement.
                        a.job.last_error =
                            Some(format!("settlement requires reconciliation: {error:#}"));
                        a.job.retry_after = None;
                    }
                }
                launch.lease = a.heartbeat.stop()?;
                if let Some(retained) = a.job.launch.as_mut() {
                    retained.lease = launch.lease.clone();
                }
                let released = w.release(&launch.lease);
                if let Err(error) = released {
                    if a.job.launch.is_some() || admission_hook.is_some() {
                        a.job.last_error = Some(format!(
                            "settlement/lease reconciliation required: {error:#}"
                        ));
                    } else {
                        storage::write(
                            &launch.log_dir.join("release-error.json"),
                            &json!({"error":error.to_string()}),
                        )?;
                    }
                }
                if admission_hook.is_some() && a.job.last_error.is_some() {
                    a.job.retry_after = None;
                    guarded_admission_failed = true;
                    if let Some(hook) = admission_hook.as_deref_mut() {
                        hook.revoke("guarded settlement requires reconciliation");
                    }
                }
                a.job_writes.persist(c, &a.job)?;
                continue;
            }
            i += 1;
        }
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
        if active.is_empty() && jobs(c)?.iter().all(|job| job.launch.is_none()) {
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
            && active.is_empty()
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
        let track = active
            .first()
            .map(|a| a.job.task.exploratory)
            .unwrap_or(prefer_exploration);
        let mut pending = jobs(c)?;
        let may_admit = active.is_empty() && pending.iter().all(|job| job.launch.is_none());
        pending.retain(|job| {
            scope
                .as_ref()
                .is_none_or(|scope| scope.ids.contains(&job.task.id))
        });
        pending.sort_by_key(|j| j.task.exploratory != track);
        let mut admitted = false;
        let mut retry_waiting = false;
        // Finish the selected cohort's prerequisites before the first lease.
        // A failed prerequisite takes no capacity; later ready peers can join.
        let mut prepared: Vec<Job> = Vec::new();
        let mut deferred_admission = false;
        let mut checked_cohort: Option<String> = None;
        let admission_deadline =
            process::deadline_scope(Duration::from_secs(window.remaining(c, &ledger)));
        for mut job in pending {
            let mut pending_writes = admission::JobWrites::new(&job, admission_hook.is_some());
            if !may_admit {
                break;
            }
            if prepared.len() >= c.max_agents {
                deferred_admission = true;
                break;
            }
            if window.remaining(c, &ledger) == 0 {
                break;
            }
            if job.launch.is_some() {
                continue;
            }
            // Sandbox verification stays in launch, with no earlier peer.
            // Do not reorder it ahead of an already selected ordinary task.
            if prepared.first().is_some_and(|first| {
                job.task.exploratory != first.task.exploratory
                    || job
                        .task
                        .required_tools
                        .iter()
                        .any(|name| name == "repo-sandbox")
            }) {
                deferred_admission = true;
                continue;
            }
            if let Some(due) = job.retry_after {
                ensure!(
                    admission_hook.is_none(),
                    "guarded admission cannot automatically retry an earlier attempt"
                );
                retry_waiting = true;
                if due > time {
                    continue;
                }
                if prepare_prerequisite(c, &mut ledger, window, false, || {
                    prepare_retry(c, &mut job)
                })?
                .is_none()
                {
                    break;
                }
                last = now();
            }
            if job.last_error.is_some() {
                continue;
            }
            let eligible = prepare_prerequisite(c, &mut ledger, window, false, || {
                admission_ready(c, &w, &job)
            })?;
            last = now();
            match eligible {
                Some(true) => {}
                Some(false) => continue,
                None => break,
            }
            let cohort = job
                .research_inputs
                .as_ref()
                .map(inputs::cohort)
                .unwrap_or_default();
            if let Some(checked) = &checked_cohort {
                if checked != &cohort {
                    deferred_admission = true;
                    continue;
                }
            } else {
                let checked = prepare_prerequisite(c, &mut ledger, window, false, || {
                    let _deadline = process::deadline_scope(Duration::from_secs(60));
                    inputs::verify(c, job.research_inputs.as_ref())
                });
                last = now();
                if matches!(checked, Ok(None)) {
                    break;
                }
                if let Err(error) = checked {
                    job.last_error = Some(format!(
                        "latest-baseline admission blocked; preserve this experiment and enqueue a fresh cohort: {error:#}"
                    ));
                    job.retry_after = None;
                    pending_writes.persist(c, &job)?;
                    continue;
                }
                checked_cohort = Some(cohort);
            }
            if !desktop_prepared
                && job
                    .task
                    .required_tools
                    .iter()
                    .any(|name| name == "computer-use-cli")
            {
                // Cold preparation may download dependencies for up to 55 s.
                // Never run it under the 10 s allowance of an active cohort.
                retry_waiting = true;
                if now() < next_desktop_prepare {
                    continue;
                }
                let prepared =
                    prepare_prerequisite(c, &mut ledger, window, false, || targets::prepare(c));
                last = now();
                let diagnostic = c.state_dir.join("target-preparation-error.json");
                match prepared {
                    Ok(Some(_)) => {
                        desktop_prepared = true;
                        if diagnostic.exists() {
                            fs::remove_file(&diagnostic)?;
                        }
                    }
                    Ok(None) => break,
                    Err(error) => {
                        next_desktop_prepare = now() + 30;
                        storage::write(
                            &diagnostic,
                            &json!({"at":now(),"state":ExecutionState::WaitingTarget,
                                "retry_after":next_desktop_prepare,"error":format!("{error:#}")}),
                        )?;
                        // No workflow attempt has been claimed. The next idle
                        // interval may safely resume dependency preparation.
                        continue;
                    }
                }
            }
            let build_logs = c.state_dir.join("runs").join(&job.run_id);
            if (job.task.write
                || job
                    .task
                    .required_tools
                    .iter()
                    .any(|name| name == "qualitygate-cli"))
                && build_cache::applicable(&job.worktree)
                && !build_cache::ready(&job.worktree, &build_logs).unwrap_or(false)
            {
                // Dependency copies can be large. Prepare only in an idle,
                // accounted host window, before acquiring/claiming any effect.
                retry_waiting = true;
                if next_build_prepare
                    .get(&job.run_id)
                    .is_some_and(|due| *due > now())
                {
                    continue;
                }
                let prepared = prepare_prerequisite(c, &mut ledger, window, false, || {
                    build_cache::prepare(&job.worktree, &build_logs)
                });
                last = now();
                match prepared {
                    Ok(Some(_)) => {
                        next_build_prepare.remove(&job.run_id);
                    }
                    Ok(None) => break,
                    Err(error) if build_cache::deferred(&error) => {
                        next_build_prepare.insert(job.run_id.clone(), now() + 30);
                        continue;
                    }
                    Err(error) => {
                        job.last_error = Some(format!(
                            "build cache preparation failed before claim: {error:#}"
                        ));
                        pending_writes.persist(c, &job)?;
                        continue;
                    }
                }
            }
            let availability = prepare_prerequisite(c, &mut ledger, window, false, || {
                // DesktopSession verifies the cached dependency manifest in
                // each worker. Availability probing never downloads packages.
                job.task.required_tools.iter().try_for_each(|name| {
                    let t = c
                        .tools
                        .get(name)
                        .context("required tool no longer configured")?;
                    let out = process::capture(
                        t.binary.to_str().context("non-UTF8 tool path")?,
                        &t.probe,
                        &c.workspace,
                        Duration::from_secs(15),
                    )?;
                    ensure!(out.status.success(), "required tool {name} unavailable");
                    Ok::<_, anyhow::Error>(())
                })
            });
            last = now();
            if matches!(availability, Ok(None)) {
                break;
            }
            if let Err(error) = availability {
                // Missing bindings block this task until explicit retry/config repair.
                job.last_error = Some(error.to_string());
                job.retry_after = None;
                pending_writes.persist(c, &job)?;
                continue;
            }
            prepared.push(job);
        }
        // Preparation and probes can be slow even when caches are warm. Check
        // latest inputs again while still lease-free, then retain local launch
        // checks for every task's frozen bytes, prompt, config and dependencies.
        if let Some(first) = prepared.first() {
            let checked = prepare_prerequisite(c, &mut ledger, window, false, || {
                inputs::verify(c, first.research_inputs.as_ref())
            });
            last = now();
            match checked {
                Ok(Some(())) => {}
                Ok(None) => prepared.clear(),
                Err(error) => {
                    for job in &mut prepared {
                        let mut writes = admission::JobWrites::new(job, admission_hook.is_some());
                        job.last_error = Some(format!(
                            "latest-baseline admission blocked; preserve this experiment and enqueue a fresh cohort: {error:#}"
                        ));
                        job.retry_after = None;
                        writes.persist(c, job)?;
                    }
                    prepared.clear();
                }
            }
        }
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
            if active.len() >= c.max_agents {
                break;
            }
            if active.first().is_some_and(|first| {
                job.task.exploratory != first.job.task.exploratory
                    || job
                        .task
                        .required_tools
                        .iter()
                        .any(|name| name == "repo-sandbox")
            }) {
                deferred_admission = true;
                continue;
            }
            let eligible =
                prepare_prerequisite(c, &mut ledger, window, !active.is_empty(), || {
                    admission_ready(c, &w, &job)
                })?;
            if active.is_empty() {
                last = now();
            }
            match eligible {
                Some(true) => {}
                Some(false) => continue,
                None => break,
            }
            ledger.budget.tick(now(), !active.is_empty())?;
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
            let launched = prepare_host(c, &mut ledger, window, !active.is_empty(), || {
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
            if active.is_empty() {
                last = now();
            }
            match launched {
                Ok(Some((process, heartbeat))) => {
                    active.push(Active {
                        job,
                        job_writes,
                        process,
                        heartbeat,
                    });
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
        if !active.is_empty() && ledger.budget.active_since.is_none() {
            ledger.budget.tick(now(), true)?;
            storage::write(&ledger_path, &ledger)?;
        }
        if active.is_empty() && guarded_admission_failed {
            break;
        }
        if active.is_empty() && !admitted {
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
            std::thread::sleep(Duration::from_millis(200));
        } else {
            std::thread::sleep(Duration::from_millis(200));
        }
    }
    scoped_run_status(c, scope.as_ref(), &deferred_followups)
}

/// Recheck durable admission before preparation and again before a claim.
/// Dependency errors or future protocol states cannot authorize execution.
fn admission_ready(c: &Config, w: &Workflow, job: &Job) -> Result<bool> {
    let state = w.status(&job.run_id)?;
    if WorkflowState::from_value(&state["status"]) != Some(WorkflowState::Running)
        || state.get("pause").is_some_and(|value| !value.is_null())
    {
        return Ok(false);
    }
    Ok(!job.task.dependencies.iter().any(|id| {
        storage::read::<Job>(&job_path(c, id))
            .and_then(|dependency| w.status(&dependency.run_id))
            .map(|state| {
                WorkflowState::from_value(&state["status"]) != Some(WorkflowState::Succeeded)
            })
            .unwrap_or(true)
    }))
}
