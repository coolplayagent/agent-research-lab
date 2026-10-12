//! Bounded process groups. Arguments are never interpreted by a shell.
use anyhow::{Context, Result, bail};
use native_effects::fs::{self, File, OpenOptions};
use std::{
    io::Read,
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Output, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
mod observer;
#[cfg(test)]
mod observer_tests;
pub use observer::{
    CaptureObserver, CaptureRequest, CaptureReservation, CapturedBytes, CapturedControl,
    ObserverContext, ObserverGuard, cleanup_scope, observer_scope,
};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);
const MAX_OUTPUT: u64 = 16 * 1024 * 1024;
thread_local! { static DEADLINE: std::cell::Cell<Option<Instant>> = const { std::cell::Cell::new(None) }; }
pub struct DeadlineGuard(Option<Instant>);
impl Drop for DeadlineGuard {
    fn drop(&mut self) {
        DEADLINE.set(self.0);
    }
}
/// Bound a sequence of host adapter calls by one wall-clock deadline.
pub fn deadline_scope(duration: Duration) -> DeadlineGuard {
    let previous = DEADLINE.get();
    let next = Instant::now() + duration;
    DEADLINE.set(Some(previous.map_or(next, |old| old.min(next))));
    DeadlineGuard(previous)
}

/// Whether the enclosing host-operation allowance has expired. Callers may
/// defer replay-safe work instead of turning an exhausted budget into rejection.
pub fn deadline_exhausted() -> bool {
    DEADLINE
        .get()
        .is_some_and(|deadline| Instant::now() >= deadline)
}

/// Bounded independent host checks. Every child inherits the same absolute
/// deadline, and all started checks are joined before any error is returned.
pub fn parallel_map<T: Sync, R: Send>(
    items: &[T],
    operation: impl Fn(&T) -> Result<R> + Sync,
) -> Result<Vec<R>> {
    let deadline = DEADLINE.get();
    let observer_context = ObserverContext::current();
    let mut output = Vec::with_capacity(items.len());
    for batch in items.chunks(4) {
        observer_context.check_work()?;
        if deadline_exhausted() {
            bail!("host operation wall-clock budget exhausted");
        }
        let results = std::thread::scope(|scope| {
            let handles: Vec<_> = batch
                .iter()
                .map(|item| {
                    let operation = &operation;
                    let observer_context = observer_context.clone();
                    scope.spawn(move || {
                        let _observer = observer_context.enter();
                        DEADLINE.set(deadline);
                        operation(item)
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|handle| {
                    handle
                        .join()
                        .unwrap_or_else(|_| Err(anyhow::anyhow!("parallel host check panicked")))
                })
                .collect::<Vec<_>>()
        });
        // Collect only after joining every child, including siblings of failures.
        output.extend(results.into_iter().collect::<Result<Vec<_>>>()?);
    }
    Ok(output)
}

pub struct Process {
    child: Child,
    pub stdout: PathBuf,
    pub stderr: PathBuf,
    started: Instant,
    timeout: Duration,
    settled: bool,
    actual_status: Option<ExitStatus>,
    stdout_guard: Option<File>,
    stderr_guard: Option<File>,
}
struct SpawnOptions<'a> {
    timeout: Duration,
    environment: &'a [(String, String)],
    stdin: Option<&'a Path>,
    retain_streams: bool,
}

impl Process {
    pub fn spawn(
        program: &str,
        args: &[String],
        cwd: &Path,
        directory: &Path,
        timeout: Duration,
        env: &[(String, String)],
        stdin: Option<&Path>,
    ) -> Result<Self> {
        Self::spawn_command(
            Command::new(program),
            args,
            cwd,
            directory,
            timeout,
            env,
            stdin,
        )
    }
    /// Start with an empty inherited environment; only explicitly selected variables are passed.
    pub fn spawn_clean(
        program: &str,
        args: &[String],
        cwd: &Path,
        directory: &Path,
        timeout: Duration,
        env: &[(String, String)],
        stdin: Option<&Path>,
    ) -> Result<Self> {
        let mut command = Command::new(program);
        command.env_clear();
        Self::spawn_command(command, args, cwd, directory, timeout, env, stdin)
    }
    fn spawn_command(
        cmd: Command,
        args: &[String],
        cwd: &Path,
        directory: &Path,
        timeout: Duration,
        env: &[(String, String)],
        stdin: Option<&Path>,
    ) -> Result<Self> {
        Self::spawn_with_options(
            cmd,
            args,
            cwd,
            directory,
            SpawnOptions {
                timeout,
                environment: env,
                stdin,
                retain_streams: false,
            },
        )
    }
    fn spawn_with_options(
        mut cmd: Command,
        args: &[String],
        cwd: &Path,
        directory: &Path,
        options: SpawnOptions<'_>,
    ) -> Result<Self> {
        fs::create_dir_all(directory)?;
        let stdout = directory.join("stdout.jsonl");
        let stderr = directory.join("stderr.log");
        let stdout_file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .read(options.retain_streams)
            .open(&stdout)?;
        let stderr_file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .read(options.retain_streams)
            .open(&stderr)?;
        cmd.args(args)
            .current_dir(cwd)
            .envs(options.environment.iter().cloned())
            .process_group(0)
            .stdout(Stdio::from(stdout_file.try_clone()?))
            .stderr(Stdio::from(stderr_file.try_clone()?));
        cmd.stdin(match options.stdin {
            Some(path) => Stdio::from(File::open(path)?),
            None => Stdio::null(),
        });
        ObserverContext::current().check_work()?;
        let child = cmd.spawn().context("launch bounded process")?;
        Ok(Self {
            child,
            stdout,
            stderr,
            started: Instant::now(),
            timeout: options.timeout,
            settled: false,
            actual_status: None,
            stdout_guard: options.retain_streams.then_some(stdout_file),
            stderr_guard: options.retain_streams.then_some(stderr_file),
        })
    }
    pub fn pid(&self) -> u32 {
        self.child.id()
    }
    pub fn poll(&mut self) -> Result<Option<ExitStatus>> {
        let status = self.child.try_wait()?;
        if let Some(status) = status {
            self.actual_status.get_or_insert(status);
        }
        // Check even after a fast successful exit; otherwise an oversized final
        // burst bypasses the same bound applied to a still-running worker.
        if fs::metadata(&self.stdout)?.len() > MAX_OUTPUT
            || fs::metadata(&self.stderr)?.len() > MAX_OUTPUT
        {
            self.cancel()?;
            bail!("process output limit exceeded");
        }
        if let Some(status) = status {
            self.settled = true;
            self.kill_group();
            return Ok(Some(status));
        }
        if self.started.elapsed() >= self.timeout {
            self.cancel()?;
            bail!("process timeout; outcome requires reconciliation");
        }
        Ok(None)
    }
    fn kill_group(&self) {
        // The child starts its own process group; descendants must not survive cancellation.
        unsafe {
            libc::kill(-(self.child.id() as i32), libc::SIGKILL);
        }
    }
    pub fn cancel(&mut self) -> Result<()> {
        self.kill_group();
        let status = self.child.wait()?;
        self.actual_status.get_or_insert(status);
        self.settled = true;
        Ok(())
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        if !self.settled {
            let _ = self.cancel();
        }
    }
}

pub fn capture(program: &str, args: &[String], cwd: &Path, timeout: Duration) -> Result<Output> {
    capture_env(program, args, cwd, timeout, &[])
}

/// Per-call environment overrides for task-owned tool homes, without mutating
/// the controller environment or leaking another concurrent task's settings.
pub fn capture_env(
    program: &str,
    args: &[String],
    cwd: &Path,
    timeout: Duration,
    env: &[(String, String)],
) -> Result<Output> {
    let observer_context = ObserverContext::current();
    // Only an opted-in sink with its own original cleanup cutoff may outlive
    // the foreground work allowance. Legacy captures retain the old deadline.
    let timeout = if observer_context.observer().is_some()
        && observer_context.class() == contracts::NativeCaptureClass::Cleanup
    {
        timeout
    } else {
        DEADLINE.get().map_or(timeout, |d| {
            timeout.min(d.saturating_duration_since(Instant::now()))
        })
    };
    if timeout.is_zero() {
        bail!("host operation wall-clock budget exhausted");
    }
    if let Some(observer) = observer_context.observer() {
        let request = CaptureRequest {
            program: program.into(),
            argv: args.to_vec(),
            cwd: cwd.into(),
            timeout,
            class: observer_context.class(),
        };
        let reservation = observer.reserve(&request)?;
        return capture_reserved(&request, env, reservation);
    }
    let path = std::env::temp_dir().join(format!(
        "agent-lab-capture-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&path)?;
    let result = (|| {
        let mut child = Process::spawn(program, args, cwd, &path, timeout, env, None)?;
        loop {
            if let Some(status) = child.poll()? {
                let mut stdout = Vec::new();
                let mut stderr = Vec::new();
                File::open(&child.stdout)?
                    .take(MAX_OUTPUT)
                    .read_to_end(&mut stdout)?;
                File::open(&child.stderr)?
                    .take(MAX_OUTPUT)
                    .read_to_end(&mut stderr)?;
                return Ok(Output {
                    status,
                    stdout,
                    stderr,
                });
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    })();
    let _ = fs::remove_dir_all(path);
    result
}

fn capture_reserved(
    request: &CaptureRequest,
    env: &[(String, String)],
    mut reservation: Box<dyn CaptureReservation>,
) -> Result<Output> {
    use contracts::NativeCaptureState;
    use std::os::unix::process::ExitStatusExt;

    let mut observed = CapturedControl {
        state: NativeCaptureState::NotSpawned,
        pid: None,
        process_start_ticks: None,
        observed_executable: None,
        actual_exit: None,
        reaped: false,
        stdout: CapturedBytes::default(),
        stderr: CapturedBytes::default(),
        failure: None,
        cleanup_error: None,
    };
    let span = native_effects::control_scope(reservation.control_ordinal());
    if let Err(error) = span {
        observed.state = NativeCaptureState::Incomplete;
        observed.failure = Some(format!("guarded control span: {error:#}"));
        reservation.finish(&observed)?;
        return Err(error);
    }
    let _span = span?;
    let _capture_scope = native_effects::capture_scope();
    let limit = reservation.retained_stream_limit();
    let path = std::env::temp_dir().join(format!(
        "agent-lab-capture-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let mut created = false;
    let mut owned: Option<Process> = None;
    let operation = (|| -> Result<ExitStatus> {
        anyhow::ensure!(
            limit > 0 && limit <= MAX_OUTPUT,
            "invalid retained capture limit"
        );
        reservation.check_work()?;
        fs::create_dir(&path)?;
        created = true;
        let child = Process::spawn_with_options(
            Command::new(&request.program),
            &request.argv,
            &request.cwd,
            &path,
            SpawnOptions {
                timeout: request.timeout,
                environment: env,
                stdin: None,
                retain_streams: true,
            },
        )?;
        observed.pid = Some(child.pid());
        #[cfg(test)]
        observer_tests::after_spawn_before_observation(&child);
        // Only this actual, still-unreaped Child is inspected; no PID discovery.
        observed.process_start_ticks = owned_start_ticks(child.pid()).ok();
        observed.observed_executable = fs::read_link(format!("/proc/{}/exe", child.pid())).ok();
        owned = Some(child);
        reservation.child_spawned(
            observed.pid.context("capture child identity missing")?,
            observed.process_start_ticks,
        )?;
        loop {
            reservation.check_work()?;
            let child = owned.as_mut().context("capture child missing")?;
            if fs::metadata(&child.stdout)?.len() > limit
                || fs::metadata(&child.stderr)?.len() > limit
            {
                observed.state = NativeCaptureState::Overflow;
                bail!("observed process output limit exceeded");
            }
            if let Some(status) = child.poll()? {
                observed.state = if status.signal().is_some() {
                    NativeCaptureState::Signal
                } else {
                    NativeCaptureState::Exited
                };
                return Ok(status);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    })();
    if let Err(error) = &operation {
        observed.failure = Some(format!("{error:#}"));
        if observed.pid.is_some() && observed.state != NativeCaptureState::Overflow {
            observed.state = NativeCaptureState::Incomplete;
        }
    }
    if let Some(child) = owned.as_mut() {
        if let Err(error) = reservation.check_cleanup() {
            observed.cleanup_error = Some(format!("cleanup clock: {error:#}"));
        }
        // Always contain the actual owned child. Late/failed cleanup cannot pass.
        if !child.settled
            && let Err(error) = child.cancel()
        {
            observed.cleanup_error = Some(format!("actual child cleanup: {error:#}"));
        }
        observed.actual_exit = child.actual_status.map(|status| {
            status
                .code()
                .unwrap_or_else(|| -status.signal().unwrap_or(0))
        });
        observed.reaped = child.actual_status.is_some();
        #[cfg(test)]
        observer_tests::before_retention(&child.stdout, &child.stderr);
        observed.stdout = read_captured(&child.stdout, child.stdout_guard.as_ref(), limit);
        observed.stderr = read_captured(&child.stderr, child.stderr_guard.as_ref(), limit);
        if observed.stdout.truncated || observed.stderr.truncated {
            observed.state = NativeCaptureState::Overflow;
        }
        if (!observed.reaped
            || observed.cleanup_error.is_some()
            || observed.process_start_ticks.is_none()
            || observed.observed_executable.is_none()
            || !observed.stdout.complete
            || !observed.stderr.complete)
            && observed.state != NativeCaptureState::Overflow
        {
            observed.state = NativeCaptureState::Incomplete;
        }
    }
    if let Err(error) = reservation.check_cleanup() {
        observed.cleanup_error = Some(format!("final capture clock: {error:#}"));
        observed.state = NativeCaptureState::Incomplete;
    }
    // A reservation's error never prevents cleanup or erases the first actual wait.
    let publication = reservation.finish(&observed);
    let removal = if created {
        let cleanup_needed = operation.is_err()
            || publication.is_err()
            || ObserverContext::current().check_work().is_err();
        let _cleanup = cleanup_needed.then(cleanup_scope);
        let removal = fs::remove_dir_all(&path);
        if removal.is_err() && !cleanup_needed && ObserverContext::current().normal_revoked() {
            // Only this source-created scratch directory is eligible. A failed normal
            // reservation must not consume the independent containment allowance.
            // Retain the original failure even if the remaining owned entries are removed.
            let _cleanup = cleanup_scope();
            let _ = fs::remove_dir_all(&path);
        }
        removal
    } else {
        Ok(())
    };
    drop(owned);
    publication?;
    removal.context("remove owned temporary capture")?;
    reservation
        .check_work()
        .context("capture finalized after its original work cutoff or stop")?;
    reservation
        .check_cleanup()
        .context("capture finalized after its original cleanup cutoff")?;
    let status = operation?;
    anyhow::ensure!(
        observed.reaped
            && observed.cleanup_error.is_none()
            && observed.process_start_ticks.is_some()
            && observed.observed_executable.is_some()
            && matches!(
                observed.state,
                NativeCaptureState::Exited | NativeCaptureState::Signal
            )
            && observed.stdout.complete
            && observed.stderr.complete,
        "observed native capture incomplete"
    );
    Ok(Output {
        status,
        stdout: observed.stdout.retained,
        stderr: observed.stderr.retained,
    })
}

fn owned_start_ticks(pid: u32) -> Result<u64> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat"))?;
    let fields: Vec<_> = stat
        .rsplit_once(')')
        .context("owned child stat shape")?
        .1
        .split_whitespace()
        .collect();
    fields
        .get(19)
        .context("owned child stat start")?
        .parse()
        .map_err(Into::into)
}

fn read_captured(path: &Path, retained: Option<&File>, limit: u64) -> CapturedBytes {
    use std::os::unix::fs::{FileExt, MetadataExt};
    let mut output = CapturedBytes::default();
    let result = (|| -> Result<()> {
        let file = retained.context("original capture descriptor unavailable")?;
        let before = file.metadata()?;
        anyhow::ensure!(
            before.is_file() && before.nlink() == 1,
            "capture regular shape"
        );
        output.observed_file_bytes = Some(before.len());
        let mut offset = 0;
        let mut buffer = [0u8; 65536];
        while offset <= limit {
            let available = usize::try_from((limit + 1 - offset).min(buffer.len() as u64))?;
            let count = file.read_at(&mut buffer[..available], offset)?;
            if count == 0 {
                break;
            }
            output.retained.extend_from_slice(&buffer[..count]);
            offset += count as u64;
        }
        output.truncated = before.len() > limit || output.retained.len() as u64 > limit;
        output.retained.truncate(limit as usize);
        let after = file.metadata()?;
        let current = fs::symlink_metadata(path)?;
        let key = |m: &fs::Metadata| {
            (
                m.dev(),
                m.ino(),
                m.len(),
                m.mtime(),
                m.mtime_nsec(),
                m.ctime(),
                m.ctime_nsec(),
            )
        };
        anyhow::ensure!(
            key(&before) == key(&after) && key(&before) == key(&current),
            "capture file changed"
        );
        output.complete = !output.truncated && output.retained.len() as u64 == before.len();
        Ok(())
    })();
    if let Err(error) = result {
        output.error = Some(format!("{error:#}"));
        output.complete = false;
    }
    output
}

pub fn checked(program: &str, args: &[String], cwd: &Path) -> Result<String> {
    let output = capture(program, args, cwd, Duration::from_secs(120))?;
    if !output.status.success() {
        bail!(
            "{program} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn literal_args_and_exit_status() {
        let t = tempfile::tempdir().unwrap();
        let o = capture(
            "printf",
            &["%s".into(), "$(touch forbidden)".into()],
            t.path(),
            Duration::from_secs(2),
        )
        .unwrap();
        assert_eq!(o.stdout, b"$(touch forbidden)");
        assert!(!t.path().join("forbidden").exists());
        assert!(
            !capture("false", &[], t.path(), Duration::from_secs(2))
                .unwrap()
                .status
                .success()
        );
    }
    #[test]
    fn fast_successful_exit_does_not_bypass_output_limit() {
        let temp = tempfile::tempdir().unwrap();
        let mut process = Process::spawn(
            "/usr/bin/head",
            &[
                "-c".into(),
                (MAX_OUTPUT + 1).to_string(),
                "/dev/zero".into(),
            ],
            temp.path(),
            &temp.path().join("logs"),
            Duration::from_secs(5),
            &[],
            None,
        )
        .unwrap();
        let started = Instant::now();
        while process.child.try_wait().unwrap().is_none() {
            assert!(started.elapsed() < Duration::from_secs(5));
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            process
                .poll()
                .unwrap_err()
                .to_string()
                .contains("output limit")
        );
    }
    #[test]
    fn deadline_terminates_child() {
        let t = tempfile::tempdir().unwrap();
        assert!(capture("sleep", &["5".into()], t.path(), Duration::from_millis(20)).is_err());
    }
}

#[cfg(test)]
mod parallel_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn independent_checks_are_bounded_and_return_in_input_order() {
        let active = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);
        let items: Vec<_> = (0..11).collect();
        let result = parallel_map(&items, |item| {
            let current = active.fetch_add(1, Ordering::SeqCst) + 1;
            peak.fetch_max(current, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(20 + (3 - item % 4) * 5));
            active.fetch_sub(1, Ordering::SeqCst);
            Ok(item * 2)
        })
        .unwrap();
        assert_eq!(
            result,
            items.iter().map(|item| item * 2).collect::<Vec<_>>()
        );
        assert!((2..=4).contains(&peak.load(Ordering::SeqCst)));
        assert_eq!(active.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn failures_and_panics_join_started_siblings_without_starting_later_batches() {
        for panic in [false, true] {
            let completed = AtomicUsize::new(0);
            let started = AtomicUsize::new(0);
            let result = parallel_map(&[0, 1, 2, 3, 4], |item| {
                started.fetch_add(1, Ordering::SeqCst);
                if *item == 0 {
                    assert!(!panic, "fixture worker panic");
                    bail!("fixture unavailable source");
                }
                std::thread::sleep(Duration::from_millis(30));
                completed.fetch_add(1, Ordering::SeqCst);
                Ok(())
            });
            assert!(result.is_err());
            assert_eq!(started.load(Ordering::SeqCst), 4);
            assert_eq!(completed.load(Ordering::SeqCst), 3);
        }
    }

    #[test]
    fn later_batches_share_the_original_host_deadline() {
        let temp = tempfile::tempdir().unwrap();
        let started = Instant::now();
        let _deadline = deadline_scope(Duration::from_millis(250));
        let calls = AtomicUsize::new(0);
        let result = parallel_map(&[0, 1, 2, 3, 4, 5, 6, 7, 8], |item| {
            calls.fetch_add(1, Ordering::SeqCst);
            capture(
                "/bin/sleep",
                &[if *item < 4 { "0.10" } else { "5" }.into()],
                temp.path(),
                Duration::from_secs(10),
            )
        });
        assert!(result.is_err());
        assert!(deadline_exhausted());
        assert!(started.elapsed() < Duration::from_secs(2));
        assert!(calls.load(Ordering::SeqCst) <= 8);
        assert!(parallel_map(&[1], |_| -> Result<()> { panic!("expired work started") }).is_err());
    }
}
