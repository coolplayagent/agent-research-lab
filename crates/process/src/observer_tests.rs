use super::*;
use contracts::{NativeCaptureClass, NativeCaptureState};
use std::{
    io::Write,
    sync::{Arc, Mutex, atomic::AtomicBool},
};

#[derive(Clone)]
struct Recorder {
    records: Arc<Mutex<Vec<(CaptureRequest, CapturedControl)>>>,
    stop_after_spawn: bool,
    finish_error: bool,
    limit: u64,
    deadline: Instant,
    cleanup_deadline: Instant,
    stop_on_finish: bool,
    finish_delay: Duration,
}
struct Reservation {
    observer: Recorder,
    request: CaptureRequest,
    stop: AtomicBool,
}
impl CaptureObserver for Recorder {
    fn reserve(&self, request: &CaptureRequest) -> Result<Box<dyn CaptureReservation>> {
        Ok(Box::new(Reservation {
            observer: self.clone(),
            request: request.clone(),
            stop: AtomicBool::new(false),
        }))
    }
}
impl CaptureReservation for Reservation {
    fn check_work(&self) -> Result<()> {
        anyhow::ensure!(!self.stop.load(Ordering::SeqCst), "fixture original stop");
        anyhow::ensure!(
            Instant::now() < self.observer.deadline,
            "fixture original deadline"
        );
        Ok(())
    }
    fn check_cleanup(&self) -> Result<()> {
        anyhow::ensure!(
            Instant::now() < self.observer.cleanup_deadline,
            "fixture original cleanup deadline"
        );
        Ok(())
    }
    fn retained_stream_limit(&self) -> u64 {
        self.observer.limit
    }
    fn child_spawned(&mut self, _: u32, _: Option<u64>) -> Result<()> {
        self.stop
            .store(self.observer.stop_after_spawn, Ordering::SeqCst);
        Ok(())
    }
    fn finish(&mut self, observed: &CapturedControl) -> Result<()> {
        self.observer
            .records
            .lock()
            .unwrap()
            .push((self.request.clone(), observed.clone()));
        std::thread::sleep(self.observer.finish_delay);
        if self.observer.stop_on_finish {
            self.stop.store(true, Ordering::SeqCst);
        }
        anyhow::ensure!(!self.observer.finish_error, "fixture publication failed");
        Ok(())
    }
}
fn recorder(limit: u64, stop: bool, publication_failure: bool) -> Recorder {
    Recorder {
        records: Arc::new(Mutex::new(Vec::new())),
        stop_after_spawn: stop,
        finish_error: publication_failure,
        limit,
        deadline: Instant::now() + Duration::from_secs(5),
        cleanup_deadline: Instant::now() + Duration::from_secs(5),
        stop_on_finish: false,
        finish_delay: Duration::ZERO,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Fault {
    None,
    WaitBeforeIdentity,
    ReplaceBeforeRetention,
}
thread_local! { static FAULT: std::cell::Cell<Fault> = const { std::cell::Cell::new(Fault::None) }; }
pub(super) fn after_spawn_before_observation(child: &Process) {
    if FAULT.get() == Fault::WaitBeforeIdentity {
        // Observe actual exit without reaping this exact owned Child. Its exe
        // becomes unavailable while the PID/start and later actual wait remain.
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        assert_eq!(
            unsafe {
                libc::waitid(
                    libc::P_PID,
                    child.pid(),
                    &mut info,
                    libc::WEXITED | libc::WNOWAIT,
                )
            },
            0
        );
    }
}
pub(super) fn before_retention(stdout: &Path, _: &Path) {
    if FAULT.get() == Fault::ReplaceBeforeRetention {
        let original = std::fs::read(stdout).unwrap();
        std::fs::rename(stdout, stdout.with_file_name("stdout.original")).unwrap();
        std::fs::write(stdout, original).unwrap();
    }
}
#[test]
fn unavailable_actual_executable_and_replaced_capture_inode_cannot_return_success() {
    let root = tempfile::tempdir().unwrap();
    for fault in [Fault::WaitBeforeIdentity, Fault::ReplaceBeforeRetention] {
        let observer = recorder(65535, false, false);
        let _guard = observer_scope(Arc::new(observer.clone()));
        FAULT.set(fault);
        let result = fixture("zero", root.path());
        FAULT.set(Fault::None);
        assert!(result.is_err());
        let records = observer.records.lock().unwrap();
        let actual = &records[0].1;
        assert_eq!(actual.actual_exit, Some(0));
        assert!(actual.reaped);
        assert_eq!(actual.state, NativeCaptureState::Incomplete);
        if fault == Fault::WaitBeforeIdentity {
            assert!(actual.observed_executable.is_none());
        } else {
            assert!(
                actual
                    .stdout
                    .error
                    .as_ref()
                    .unwrap()
                    .contains("capture file changed")
            );
        }
    }
}
#[test]
fn late_publication_stop_and_original_normal_allowance_are_checked() {
    let root = tempfile::tempdir().unwrap();
    let mut observer = recorder(65535, false, false);
    observer.stop_on_finish = true;
    {
        let _guard = observer_scope(Arc::new(observer.clone()));
        assert!(
            fixture("zero", root.path())
                .unwrap_err()
                .to_string()
                .contains("original work cutoff or stop")
        );
    }
    assert_eq!(observer.records.lock().unwrap()[0].1.actual_exit, Some(0));
    let observer = recorder(65535, false, false);
    let _guard = observer_scope(Arc::new(observer.clone()));
    let _normal_deadline = deadline_scope(Duration::ZERO);
    assert!(fixture("zero", root.path()).is_err());
    assert!(observer.records.lock().unwrap().is_empty());
    {
        let _cleanup = cleanup_scope();
        fixture("zero", root.path()).unwrap();
    }
    assert_eq!(
        observer.records.lock().unwrap()[0].0.class,
        NativeCaptureClass::Cleanup
    );
    assert_eq!(observer.records.lock().unwrap()[0].1.actual_exit, Some(0));
    assert!(deadline_exhausted());
}
#[test]
fn publication_crossing_original_cleanup_cutoff_preserves_wait_and_rejects_success() {
    let root = tempfile::tempdir().unwrap();
    let mut observer = recorder(65535, false, false);
    observer.cleanup_deadline = Instant::now() + Duration::from_millis(150);
    observer.finish_delay = Duration::from_millis(150);
    let _guard = observer_scope(Arc::new(observer.clone()));
    let result = fixture("zero", root.path()).unwrap_err();
    assert!(format!("{result:#}").contains("original cleanup cutoff"));
    let records = observer.records.lock().unwrap();
    assert_eq!(records[0].1.actual_exit, Some(0));
    assert!(records[0].1.reaped);
}
fn fixture(mode: &str, root: &Path) -> Result<Output> {
    capture_env(
        std::env::current_exe()?.to_str().unwrap(),
        &[
            "--exact".into(),
            "observer_tests::fixture_child".into(),
            "--nocapture".into(),
        ],
        root,
        Duration::from_secs(5),
        &[("LAB_CAPTURE_FIXTURE".into(), mode.into())],
    )
}
#[test]
fn fixture_child() {
    let Ok(mode) = std::env::var("LAB_CAPTURE_FIXTURE") else {
        return;
    };
    std::thread::sleep(Duration::from_millis(80));
    match mode.as_str() {
        "zero" => {
            std::io::stdout().write_all(b"raw\xff\0bytes").unwrap();
        }
        "seven" => std::process::exit(7),
        "overflow" => {
            std::io::stdout().write_all(&vec![b'x'; 8193]).unwrap();
        }
        "sleep" => std::thread::sleep(Duration::from_secs(5)),
        _ => panic!("unknown owned fixture"),
    }
}
#[test]
fn raw_bytes_actual_zero_and_seven_are_preserved_before_removal() {
    let root = tempfile::tempdir().unwrap();
    for (mode, expected_exit) in [("zero", 0), ("seven", 7)] {
        let observer = recorder(65535, false, false);
        let _guard = observer_scope(Arc::new(observer.clone()));
        let output = fixture(mode, root.path()).unwrap();
        assert_eq!(output.status.code(), Some(expected_exit));
        let records = observer.records.lock().unwrap();
        assert_eq!(records.len(), 1);
        let (request, actual) = &records[0];
        assert_eq!(request.class, NativeCaptureClass::Normal);
        assert!(actual.reaped && actual.pid.is_some() && actual.process_start_ticks.is_some());
        assert_eq!(actual.actual_exit, Some(expected_exit));
        assert_eq!(actual.stdout.retained, output.stdout);
        assert_eq!(actual.stderr.retained, output.stderr);
        assert!(actual.stdout.complete && actual.stderr.complete);
        if mode == "zero" {
            assert!(
                actual
                    .stdout
                    .retained
                    .windows(10)
                    .any(|b| b == b"raw\xff\0bytes")
            );
        }
    }
}
#[test]
fn overflow_stop_and_publication_failure_do_not_erase_actual_wait() {
    let root = tempfile::tempdir().unwrap();
    for (mode, limit, stop, publication_failure) in [
        ("overflow", 4096, false, false),
        ("sleep", 65535, true, false),
        ("zero", 65535, false, true),
    ] {
        let observer = recorder(limit, stop, publication_failure);
        let _guard = observer_scope(Arc::new(observer.clone()));
        assert!(fixture(mode, root.path()).is_err());
        let records = observer.records.lock().unwrap();
        assert_eq!(records.len(), 1);
        let actual = &records[0].1;
        assert!(actual.reaped && actual.actual_exit.is_some());
        assert!(actual.stdout.retained.len() as u64 <= limit);
        if mode == "overflow" {
            assert!(actual.stdout.truncated && !actual.stdout.complete);
            assert_eq!(actual.state, NativeCaptureState::Overflow);
            assert!(actual.stdout.observed_file_bytes.unwrap() > limit);
        }
        if stop {
            assert!(
                actual
                    .failure
                    .as_ref()
                    .unwrap()
                    .contains("fixture original stop")
            );
            assert_eq!(actual.state, NativeCaptureState::Incomplete);
        }
        if publication_failure {
            assert_eq!(actual.actual_exit, Some(0));
        }
    }
}
#[test]
fn parallel_checks_inherit_the_same_observer_and_cleanup_class() {
    let root = tempfile::tempdir().unwrap();
    let observer = recorder(65535, false, false);
    {
        let _guard = observer_scope(Arc::new(observer.clone()));
        let _cleanup = cleanup_scope();
        parallel_map(&[0, 1, 2, 3], |_| fixture("zero", root.path())).unwrap();
    }
    let records = observer.records.lock().unwrap();
    assert_eq!(records.len(), 4);
    assert!(records.iter().all(
        |(request, actual)| request.class == NativeCaptureClass::Cleanup
            && actual.actual_exit == Some(0)
            && actual.reaped
    ));
    assert_eq!(
        records
            .iter()
            .map(|(_, actual)| actual.pid.unwrap())
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        4
    );
    drop(records);
    let prior_count = observer.records.lock().unwrap().len();
    fixture("zero", root.path()).unwrap();
    assert_eq!(observer.records.lock().unwrap().len(), prior_count);
}
