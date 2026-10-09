//! Renew execution authority independently of foreground host work.
//!
//! Workflow replaces the entire lease token on renewal. The mutex serializes only
//! bounded authority commands; report validation and candidate capture run outside
//! it. The separate host journal never races with foreground Job persistence.
use crate::{process, storage, workflow::Workflow};
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

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
        let thread = thread::Builder::new()
            .name("workflow-lease".into())
            .spawn(move || {
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
        let remaining = expires.saturating_sub(crate::workflow::now_ms()?);
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
    crate::config::safe_id(run)?;
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
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn lease(ttl: u64) -> Value {
        json!({"run_id":"run-1","owner":"test","acquisition_id":"acquire-1",
            "epoch":1,"issued_at_unix_ms":crate::workflow::now_ms().unwrap(),
            "expires_at_unix_ms":crate::workflow::now_ms().unwrap()+ttl})
    }

    #[test]
    fn renews_while_foreground_is_blocked_and_serializes_current_tokens() {
        let directory = tempfile::tempdir().unwrap();
        let w = Workflow::new("unused", directory.path().join("db"), directory.path());
        let original = lease(300);
        let authority = Arc::new(Mutex::new(original.clone()));
        let calls = Arc::new(AtomicUsize::new(0));
        let remote = Arc::clone(&authority);
        let renew_calls = Arc::clone(&calls);
        let mut heartbeat = LeaseHeartbeat::start_with(
            original.clone(),
            journal_path(&w, &original).unwrap(),
            Arc::new(move |token, ttl| {
                let mut current = remote.lock().unwrap();
                ensure!(*token == *current, "old token rejected");
                ensure!(
                    expiry(token)? > crate::workflow::now_ms()?,
                    "expired token rejected"
                );
                current["expires_at_unix_ms"] = json!(crate::workflow::now_ms()? + ttl);
                renew_calls.fetch_add(1, Ordering::SeqCst);
                Ok(current.clone())
            }),
            300,
            Duration::from_millis(30),
            Duration::from_millis(30),
        )
        .unwrap();
        // Several lease lifetimes elapse while the foreground does no polling.
        thread::sleep(Duration::from_millis(750));
        assert!(calls.load(Ordering::SeqCst) >= 3);
        heartbeat
            .with_lease(|token| {
                assert_eq!(*token, *authority.lock().unwrap());
                assert_ne!(*token, original);
                thread::sleep(Duration::from_millis(50));
                assert_eq!(*token, *authority.lock().unwrap());
                Ok(())
            })
            .unwrap();
        let final_token = heartbeat.stop().unwrap();
        let mut recovered = original;
        restore(&w, &mut recovered).unwrap();
        assert_eq!(final_token, recovered);
        assert_eq!(final_token, *authority.lock().unwrap());
        let count = calls.load(Ordering::SeqCst);
        thread::sleep(Duration::from_millis(80));
        assert_eq!(count, calls.load(Ordering::SeqCst));
    }

    #[test]
    fn renewal_failure_kills_worker_without_foreground_polling() {
        let directory = tempfile::tempdir().unwrap();
        let mut worker = process::Process::spawn(
            "sleep",
            &["30".into()],
            directory.path(),
            &directory.path().join("worker"),
            Duration::from_secs(30),
            &[],
            None,
        )
        .unwrap();
        let stat = fs::read_to_string(format!("/proc/{}/stat", worker.pid())).unwrap();
        let start = stat
            .rsplit_once(") ")
            .unwrap()
            .1
            .split_whitespace()
            .nth(19)
            .unwrap()
            .to_owned();
        let heartbeat = LeaseHeartbeat::start_with(
            lease(300),
            directory.path().join("lease.json"),
            Arc::new(|_, _| anyhow::bail!("database unavailable")),
            300,
            Duration::from_millis(30),
            Duration::from_millis(30),
        )
        .unwrap();
        heartbeat
            .watch_worker(worker.pid(), start, Duration::from_secs(30), None)
            .unwrap();
        thread::sleep(Duration::from_millis(200));
        let status = worker
            .poll()
            .unwrap()
            .expect("worker must stop independently");
        assert!(!status.success());
        assert!(
            heartbeat
                .failure()
                .unwrap()
                .contains("database unavailable")
        );
        assert!(heartbeat.with_lease(|_| Ok(())).is_err());
    }

    #[test]
    fn recovery_rejects_other_authority_or_older_tokens() {
        let directory = tempfile::tempdir().unwrap();
        let w = Workflow::new("unused", directory.path().join("db"), directory.path());
        let original = lease(300);
        let mut updated = original.clone();
        updated["epoch"] = json!(2);
        storage::write(&journal_path(&w, &original).unwrap(), &updated).unwrap();
        assert!(restore(&w, &mut original.clone()).is_err());
        updated = original.clone();
        updated["expires_at_unix_ms"] = json!(expiry(&original).unwrap() - 1);
        storage::write(&journal_path(&w, &original).unwrap(), &updated).unwrap();
        assert!(restore(&w, &mut original.clone()).is_err());
    }

    #[test]
    fn watchdog_enforces_timeout_and_pause_while_renewal_blocks() {
        for pause_requested in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let mut worker = process::Process::spawn(
                "sleep",
                &["30".into()],
                directory.path(),
                &directory.path().join("worker"),
                Duration::from_secs(30),
                &[],
                None,
            )
            .unwrap();
            let stat = fs::read_to_string(format!("/proc/{}/stat", worker.pid())).unwrap();
            let start = stat
                .rsplit_once(") ")
                .unwrap()
                .1
                .split_whitespace()
                .nth(19)
                .unwrap()
                .to_owned();
            let heartbeat = LeaseHeartbeat::start_with(
                lease(3_000),
                directory.path().join("lease.json"),
                Arc::new(|old, ttl| {
                    // Simulate a slow transport holding the authority mutex.
                    thread::sleep(Duration::from_millis(400));
                    let mut next = old.clone();
                    next["expires_at_unix_ms"] = json!(crate::workflow::now_ms()? + ttl);
                    Ok(next)
                }),
                3_000,
                Duration::from_millis(20),
                Duration::from_millis(500),
            )
            .unwrap();
            let pause = directory.path().join("paused");
            heartbeat
                .watch_worker(
                    worker.pid(),
                    start,
                    if pause_requested {
                        Duration::from_secs(30)
                    } else {
                        Duration::from_millis(60)
                    },
                    Some(pause.clone()),
                )
                .unwrap();
            thread::sleep(Duration::from_millis(40));
            if pause_requested {
                fs::write(pause, "pause requested").unwrap();
            }
            thread::sleep(Duration::from_millis(150));
            assert!(
                !worker
                    .poll()
                    .unwrap()
                    .expect("watchdog cancels independently")
                    .success()
            );
            let reason = heartbeat.failure().unwrap();
            assert!(reason.contains(if pause_requested { "pause" } else { "timeout" }));
            // Cancellation does not discard valid authority for an unknown-effect observation.
            heartbeat.with_lease(|_| Ok(())).unwrap();
        }
    }

    #[test]
    #[ignore = "set LAB_WORKFLOW_BIN for real workflow renew/finish/release integration"]
    fn actual_workflow_survives_blocked_foreground_and_rejects_stale_token() {
        let directory = tempfile::tempdir().unwrap();
        let w = Workflow::new(
            std::env::var("LAB_WORKFLOW_BIN").expect("LAB_WORKFLOW_BIN"),
            directory.path().join("runs.db"),
            directory.path(),
        );
        w.initialize().unwrap();
        w.start_task(
            "long-read",
            false,
            json!({"command":"/bin/sleep 4"}),
            15_000,
        )
        .unwrap();
        let lease = w
            .acquire("long-read", "heartbeat-test", "acquisition-1", 2_000)
            .unwrap();
        let renewer = w.clone();
        let mut heartbeat = LeaseHeartbeat::start_with(
            lease.clone(),
            journal_path(&w, &lease).unwrap(),
            Arc::new(move |lease, ttl| renewer.renew(lease, ttl)),
            2_000,
            Duration::from_millis(200),
            Duration::from_millis(500),
        )
        .unwrap();
        let claim = heartbeat.with_lease(|lease| w.claim(lease)).unwrap();
        let output = process::capture(
            "/bin/sleep",
            &["4.5".into()],
            directory.path(),
            Duration::from_secs(6),
        )
        .unwrap();
        assert!(output.status.success());
        assert!(
            w.release(&lease).is_err(),
            "real workflow must reject original stale token"
        );
        let receipt = heartbeat
            .with_lease(|lease| {
                w.finish(lease, &claim["attempt"],
            json!({"result":"actual sleep exited successfully across multiple lease lifetimes"}))
            })
            .unwrap();
        assert_eq!(receipt["snapshot"]["status"], "succeeded");
        let current = heartbeat.stop().unwrap();
        w.release(&current).unwrap();
        w.verify("long-read").unwrap();
    }
}
