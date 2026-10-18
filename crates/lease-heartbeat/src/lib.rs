//! Renew execution authority independently of foreground host work.
//!
//! Workflow replaces the entire lease token on renewal. The mutex serializes only
//! bounded authority commands; report validation and candidate capture run outside
//! it. The separate host journal never races with foreground Job persistence.
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use workflow::Workflow;

pub const TTL_MS: u64 = 60_000;
const INTERVAL: Duration = Duration::from_secs(10);
const COMMAND_LIMIT: Duration = Duration::from_secs(10);
type Renew = Arc<dyn Fn(&Value, u64) -> Result<Value> + Send + Sync>;

struct State {
    lease: Value,
    failure: Option<String>,
}

struct Worker {
    pid: u32,
    start: String,
    deadline: Instant,
    pause: Option<PathBuf>,
}

#[derive(Default)]
struct WorkerState {
    worker: Option<Worker>,
    cancellation: Option<String>,
}

pub struct LeaseHeartbeat {
    state: Arc<Mutex<State>>,
    stop: mpsc::Sender<()>,
    thread: Option<JoinHandle<()>>,
    worker: Arc<Mutex<WorkerState>>,
    watch_stop: mpsc::Sender<()>,
    watchdog: Option<JoinHandle<()>>,
    journal: PathBuf,
    renew: Renew,
    ttl_ms: u64,
    command_limit: Duration,
}

impl LeaseHeartbeat {
    pub fn start(w: &Workflow, lease: Value) -> Result<Self> {
        let journal = journal_path(w, &lease)?;
        let w = w.clone();
        Self::start_with(
            lease,
            journal,
            Arc::new(move |lease, ttl| w.renew(lease, ttl)),
            TTL_MS,
            INTERVAL,
            COMMAND_LIMIT,
        )
    }

    fn start_with(
        lease: Value,
        journal: PathBuf,
        renew: Renew,
        ttl_ms: u64,
        interval: Duration,
        command_limit: Duration,
    ) -> Result<Self> {
        ensure!(
            interval + command_limit + command_limit < Duration::from_millis(ttl_ms),
            "heartbeat must retain time for renewal and a foreground authority command"
        );
        storage::write(&journal, &lease)?;
        let state = Arc::new(Mutex::new(State {
            lease,
            failure: None,
        }));
        let worker = Arc::new(Mutex::new(WorkerState::default()));
        let (stop, receive) = mpsc::channel();
        let worker_state = Arc::clone(&state);
        let worker_journal = journal.clone();
        let worker_renew = Arc::clone(&renew);
        let renewal_worker = Arc::clone(&worker);
        let capture_context = process::ObserverContext::current();
        let thread = thread::Builder::new()
            .name("workflow-lease".into())
            .spawn(move || {
                let _capture_observer = capture_context.enter();
                while matches!(
                    receive.recv_timeout(interval),
                    Err(mpsc::RecvTimeoutError::Timeout)
                ) {
                    let Ok(mut state) = worker_state.lock() else {
                        break;
                    };
                    if state.failure.is_some() {
                        break;
                    }
                    if let Err(error) = renew_locked(
                        &mut state,
                        &worker_journal,
                        &worker_renew,
                        ttl_ms,
                        command_limit,
                    ) {
                        fail_closed(&mut state, &renewal_worker, error);
                        break;
                    }
                }
            })?;
        // A separate watchdog keeps deadlines/pause responsive even during a
        // bounded but slow workflow renewal or foreground settlement command.
        let (watch_stop, watch_receive) = mpsc::channel();
        let watched = Arc::clone(&worker);
        let watchdog = match thread::Builder::new()
            .name("workflow-worker-watch".into())
            .spawn(move || {
                while matches!(
                    watch_receive.recv_timeout(Duration::from_millis(50)),
                    Err(mpsc::RecvTimeoutError::Timeout)
                ) {
                    let Ok(mut state) = watched.lock() else {
                        break;
                    };
                    let reason = state.worker.as_ref().and_then(|worker| {
                    if worker.pause.as_ref().is_some_and(|path| path.exists()) {
                        Some("controller pause requested; worker outcome requires reconciliation")
                    } else if Instant::now() >= worker.deadline {
                        Some("worker timeout; outcome requires reconciliation")
                    } else { None }
                });
                    if let Some(reason) = reason {
                        state.cancellation = Some(reason.into());
                        kill_worker(&mut state);
                    }
                }
            }) {
            Ok(watchdog) => watchdog,
            Err(error) => {
                let _ = stop.send(());
                let _ = thread.join();
                return Err(error.into());
            }
        };
        Ok(Self {
            state,
            stop,
            thread: Some(thread),
            worker,
            watch_stop,
            watchdog: Some(watchdog),
            journal,
            renew,
            ttl_ms,
            command_limit,
        })
    }

    /// Commands using authority must obtain the latest token while excluding renewal.
    /// Callers must keep filesystem/network preparation outside this closure.
    pub fn with_lease<T>(&self, operation: impl FnOnce(&Value) -> Result<T>) -> Result<T> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("lease state poisoned"))?;
        ensure!(
            state.failure.is_none(),
            "lease heartbeat failed: {:?}",
            state.failure
        );
        let expires = expiry(&state.lease)?;
        let remaining = expires.saturating_sub(workflow::now_ms()?);
        if remaining <= (self.command_limit.as_millis() * 2) as u64
            && let Err(error) = renew_locked(
                &mut state,
                &self.journal,
                &self.renew,
                self.ttl_ms,
                self.command_limit,
            )
        {
            fail_closed(&mut state, &self.worker, error);
            anyhow::bail!("lease heartbeat failed: {:?}", state.failure);
        }
        let _deadline = process::deadline_scope(self.command_limit);
        operation(&state.lease)
    }

    /// Register only a process group created by Process::spawn, after durable claim.
    pub fn watch_worker(
        &self,
        pid: u32,
        start: String,
        timeout: Duration,
        pause: Option<PathBuf>,
    ) -> Result<()> {
        ensure!(
            pid > 1 && !start.is_empty(),
            "invalid heartbeat worker identity"
        );
        let state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("lease state poisoned"))?;
        ensure!(
            state.failure.is_none(),
            "lease heartbeat failed before worker launch"
        );
        let mut worker = self
            .worker
            .lock()
            .map_err(|_| anyhow::anyhow!("worker watch poisoned"))?;
        ensure!(
            worker.worker.is_none() && worker.cancellation.is_none(),
            "worker already watched"
        );
        worker.worker = Some(Worker {
            pid,
            start,
            deadline: Instant::now() + timeout,
            pause,
        });
        Ok(())
    }

    pub fn disarm_worker(&self) {
        if let Ok(mut state) = self.worker.lock() {
            state.worker = None;
        }
    }

    pub fn failure(&self) -> Option<String> {
        match self.state.lock() {
            Ok(state) => state
                .failure
                .clone()
                .or_else(|| self.worker.lock().ok()?.cancellation.clone()),
            Err(_) => Some("lease state poisoned".into()),
        }
    }

    pub fn current(&self) -> Result<Value> {
        Ok(self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("lease state poisoned"))?
            .lease
            .clone())
    }

    /// Stop before release, so a late renewal cannot invalidate its token.
    pub fn stop(&mut self) -> Result<Value> {
        let _ = self.stop.send(());
        let _ = self.watch_stop.send(());
        if let Some(watchdog) = self.watchdog.take() {
            watchdog
                .join()
                .map_err(|_| anyhow::anyhow!("worker watchdog panicked"))?;
        }
        if let Some(thread) = self.thread.take() {
            thread
                .join()
                .map_err(|_| anyhow::anyhow!("lease heartbeat panicked"))?;
        }
        self.current()
    }
}

impl Drop for LeaseHeartbeat {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

fn renew_locked(
    state: &mut State,
    journal: &Path,
    renew: &Renew,
    ttl_ms: u64,
    limit: Duration,
) -> Result<()> {
    let _deadline = process::deadline_scope(limit);
    let renewed = renew(&state.lease, ttl_ms).context("workflow lease renewal failed")?;
    validate_successor(&state.lease, &renewed)?;
    ensure!(
        expiry(&renewed)? > expiry(&state.lease)?,
        "renewal did not extend lease"
    );
    // Retain the newest token in memory even if journal persistence fails. No
    // further work is allowed after such a failure; recovery remains conservative.
    state.lease = renewed;
    storage::write(journal, &state.lease).context("persist renewed lease")
}

fn fail_closed(state: &mut State, worker: &Mutex<WorkerState>, error: anyhow::Error) {
    state.failure = Some(format!("{error:#}"));
    if let Ok(mut state) = worker.lock() {
        kill_worker(&mut state);
    }
}

fn kill_worker(state: &mut WorkerState) {
    if let Some(worker) = state.worker.take() {
        let pid = worker.pid;
        // Do not signal a process whose PID has been reused. The foreground
        // controller remains responsible for wait()/reaping and uncertainty.
        let current = fs::read_to_string(format!("/proc/{pid}/stat"));
        if current
            .ok()
            .and_then(|stat| {
                stat.rsplit_once(") ")?
                    .1
                    .split_whitespace()
                    .nth(19)
                    .map(str::to_owned)
            })
            .as_ref()
            == Some(&worker.start)
        {
            unsafe {
                libc::kill(-(pid as i32), libc::SIGKILL);
            }
        }
    }
}

fn expiry(lease: &Value) -> Result<u64> {
    lease["expires_at_unix_ms"]
        .as_u64()
        .context("lease lacks expiry")
}

fn validate_successor(original: &Value, updated: &Value) -> Result<()> {
    let mut expected = original.clone();
    let mut actual = updated.clone();
    expected
        .as_object_mut()
        .context("lease is not an object")?
        .remove("expires_at_unix_ms");
    actual
        .as_object_mut()
        .context("lease is not an object")?
        .remove("expires_at_unix_ms");
    ensure!(
        expected == actual,
        "lease journal changes execution authority"
    );
    ensure!(
        expiry(updated)? >= expiry(original)?,
        "lease journal moves expiry backwards"
    );
    Ok(())
}

fn journal_path(w: &Workflow, lease: &Value) -> Result<PathBuf> {
    let run = lease["run_id"]
        .as_str()
        .context("lease lacks run identity")?;
    config::safe_id(run)?;
    Ok(w.work_dir.join("host-leases").join(format!("{run}.json")))
}

/// Legacy launches have no journal. A retained journal can only extend exactly
/// the same authority; it never grants a new epoch or resolves an uncertain write.
pub fn restore(w: &Workflow, lease: &mut Value) -> Result<()> {
    let path = journal_path(w, lease)?;
    if path.exists() {
        let updated: Value = storage::read(&path)?;
        validate_successor(lease, &updated)?;
        *lease = updated;
    }
    Ok(())
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
