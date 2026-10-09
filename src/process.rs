//! Bounded process groups. Arguments are never interpreted by a shell.
use anyhow::{Context, Result, bail};
use std::{
    fs::{self, File, OpenOptions},
    io::Read,
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Output, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
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
    let mut output = Vec::with_capacity(items.len());
    for batch in items.chunks(4) {
        if deadline_exhausted() {
            bail!("host operation wall-clock budget exhausted");
        }
        let results = std::thread::scope(|scope| {
            let handles: Vec<_> = batch
                .iter()
                .map(|item| {
                    let operation = &operation;
                    scope.spawn(move || {
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
        fs::create_dir_all(directory)?;
        let stdout = directory.join("stdout.jsonl");
        let stderr = directory.join("stderr.log");
        let mut cmd = Command::new(program);
        cmd.args(args)
            .current_dir(cwd)
            .envs(env.iter().cloned())
            .process_group(0)
            .stdout(Stdio::from(
                OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(&stdout)?,
            ))
            .stderr(Stdio::from(
                OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(&stderr)?,
            ));
        cmd.stdin(match stdin {
            Some(path) => Stdio::from(File::open(path)?),
            None => Stdio::null(),
        });
        let child = cmd.spawn().with_context(|| format!("launch {program}"))?;
        Ok(Self {
            child,
            stdout,
            stderr,
            started: Instant::now(),
            timeout,
            settled: false,
        })
    }
    pub fn pid(&self) -> u32 {
        self.child.id()
    }
    pub fn poll(&mut self) -> Result<Option<ExitStatus>> {
        if let Some(status) = self.child.try_wait()? {
            self.settled = true;
            self.kill_group();
            return Ok(Some(status));
        }
        if self.started.elapsed() >= self.timeout {
            self.cancel()?;
            bail!("process timeout; outcome requires reconciliation");
        }
        if fs::metadata(&self.stdout)?.len() > MAX_OUTPUT
            || fs::metadata(&self.stderr)?.len() > MAX_OUTPUT
        {
            self.cancel()?;
            bail!("process output limit exceeded");
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
        self.child.wait()?;
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
    let timeout = DEADLINE.get().map_or(timeout, |d| {
        timeout.min(d.saturating_duration_since(Instant::now()))
    });
    if timeout.is_zero() {
        bail!("host operation wall-clock budget exhausted");
    }
    let path = std::env::temp_dir().join(format!(
        "agent-lab-capture-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&path)?;
    let result = (|| {
        let mut child = Process::spawn(program, args, cwd, &path, timeout, &[], None)?;
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
