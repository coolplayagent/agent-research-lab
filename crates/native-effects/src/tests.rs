use super::*;
use anyhow::{Result, bail};
use contracts::*;
use std::{
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

#[derive(Default)]
struct Fence {
    tick: AtomicU64,
    stopped: AtomicBool,
    work_expired: AtomicBool,
    cleanup_stopped: AtomicBool,
}
impl ObservationFence for Fence {
    fn now_ns(&self) -> Result<u64> {
        Ok(self.tick.fetch_add(1, Ordering::SeqCst) + 1)
    }
    fn check_work(&self) -> Result<()> {
        anyhow::ensure!(
            !self.stopped.load(Ordering::SeqCst) && !self.work_expired.load(Ordering::SeqCst),
            "test original work fence stopped"
        );
        Ok(())
    }
    fn check_cleanup(&self) -> Result<()> {
        anyhow::ensure!(
            !self.cleanup_stopped.load(Ordering::SeqCst),
            "test original cleanup cutoff"
        );
        Ok(())
    }
    fn check_finalization(&self) -> Result<()> {
        anyhow::ensure!(
            !self.stopped.load(Ordering::SeqCst),
            "test original stop authority"
        );
        self.check_cleanup()
    }
}
#[derive(Default)]
struct Sink {
    rows: Arc<Mutex<Vec<(NativeFilesystemRequest, NativeFilesystemActual)>>>,
    normal: AtomicU64,
    fail_finish: AtomicBool,
}
struct Reservation {
    request: NativeFilesystemRequest,
    rows: Arc<Mutex<Vec<(NativeFilesystemRequest, NativeFilesystemActual)>>>,
    fail: bool,
}
impl FilesystemReservation for Reservation {
    fn finish(self: Box<Self>, actual: &NativeFilesystemActual) -> Result<()> {
        self.rows
            .lock()
            .unwrap()
            .push((self.request, actual.clone()));
        anyhow::ensure!(!self.fail, "test publication failure");
        Ok(())
    }
}
impl FilesystemObserver for Sink {
    fn reserve(&self, request: &NativeFilesystemRequest) -> Result<Box<dyn FilesystemReservation>> {
        if request.class == NativeCaptureClass::Normal {
            self.normal
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |v| v.checked_sub(1))
                .map_err(|_| anyhow::anyhow!("test normal partition exhausted"))?;
        }
        Ok(Box::new(Reservation {
            request: request.clone(),
            rows: self.rows.clone(),
            fail: self.fail_finish.load(Ordering::SeqCst),
        }))
    }
}
struct Capture;
impl CaptureObserver for Capture {
    fn reserve(&self, _: &CaptureRequest) -> Result<Box<dyn CaptureReservation>> {
        bail!("no child authorized by this fixture")
    }
}
fn session(
    root: &Path,
    normal: u64,
) -> (
    NativeObservationSession,
    Arc<Sink>,
    Arc<Fence>,
    Arc<dyn CaptureObserver>,
) {
    let sink = Arc::new(Sink::default());
    sink.normal.store(normal, Ordering::SeqCst);
    let fence = Arc::new(Fence::default());
    let capture: Arc<dyn CaptureObserver> = Arc::new(Capture);
    let stat = std::fs::read_to_string("/proc/self/stat").unwrap();
    let start = stat
        .rsplit_once(") ")
        .unwrap()
        .1
        .split_whitespace()
        .nth(19)
        .unwrap()
        .parse()
        .unwrap();
    let binding = NativeFilesystemBinding {
        schema_version: 1,
        session_sha256: "1".repeat(64),
        source_sha256: "2".repeat(64),
        config_sha256: "3".repeat(64),
        allowlist_sha256: "4".repeat(64),
        pid: std::process::id(),
        process_start_ticks: start,
    };
    let policy = FilesystemPolicy {
        roots: vec![FilesystemRoot {
            id: 1,
            path: root.to_owned(),
            category: NativeFilesystemCategory::Json,
            max_object_bytes: 16 * 1024 * 1024,
        }],
        attempts: vec![NativeFilesystemAttempt {
            task_id: "task".into(),
            run_id: "run".into(),
            attempt: 1,
        }],
        excluded: vec![root.join("opaque.sqlite")],
        max_concurrent_operations: 8,
    };
    (
        NativeObservationSession::new(
            binding,
            policy,
            capture.clone(),
            sink.clone(),
            fence.clone(),
        )
        .unwrap(),
        sink,
        fence,
        capture,
    )
}
#[test]
fn finalization_distinguishes_natural_work_expiry_from_stop_and_cleanup_expiry() {
    for (stop, cleanup_expired) in [(false, false), (true, false), (false, true)] {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("owned");
        let (s, sink, fence, _) = session(d.path(), 20);
        {
            let _scope = observation_scope(&s).unwrap();
            fs::write(&path, b"owned").unwrap();
            fence.work_expired.store(true, Ordering::SeqCst);
            fence.stopped.store(stop, Ordering::SeqCst);
            let _cleanup = cleanup_scope();
            fs::remove_file(&path).unwrap();
            fence
                .cleanup_stopped
                .store(cleanup_expired, Ordering::SeqCst);
        }
        assert!(!path.exists());
        assert_eq!(
            sink.rows.lock().unwrap().last().unwrap().0.class,
            NativeCaptureClass::Cleanup
        );
        assert_eq!(
            s.finish().unwrap().completeness,
            if stop || cleanup_expired {
                NativeFilesystemCompleteness::Incomplete
            } else {
                NativeFilesystemCompleteness::Complete
            }
        );
    }
}

#[test]
fn real_create_write_and_cleanup_survive_normal_reservation_exhaustion() {
    let d = tempfile::tempdir().unwrap();
    let (s, sink, _, _) = session(d.path(), 2);
    let path = d.path().join("transient");
    {
        let _scope = observation_scope(&s).unwrap();
        fs::write(&path, b"private content must not enter metadata").unwrap();
        assert!(fs::write(d.path().join("forbidden"), b"x").is_err());
        assert!(!d.path().join("forbidden").exists());
        let _cleanup = cleanup_scope();
        fs::remove_file(&path).unwrap();
    }
    let report = s.finish().unwrap();
    assert_eq!(
        report.completeness,
        NativeFilesystemCompleteness::Incomplete
    );
    assert!(!path.exists());
    let rows = sink.rows.lock().unwrap();
    assert!(rows.iter().any(
        |(r, a)| r.operation == NativeFilesystemOperation::Write && a.written_bytes == Some(39)
    ));
    assert!(
        rows.iter()
            .any(|(r, a)| r.operation == NativeFilesystemOperation::Unlink
                && r.class == NativeCaptureClass::Cleanup
                && a.result == NativeFilesystemResult::Succeeded)
    );
    assert!(
        !serde_json::to_string(&*rows)
            .unwrap()
            .contains("private content")
    );
}
#[test]
fn existing_open_records_distinct_exclusive_failure_and_existing_success() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("lock");
    std::fs::write(&path, b"before").unwrap();
    let (s, sink, _, _) = session(d.path(), 10);
    {
        let _scope = observation_scope(&s).unwrap();
        let _f = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&path)
            .unwrap();
    }
    assert_eq!(
        s.finish().unwrap().completeness,
        NativeFilesystemCompleteness::Complete
    );
    let rows = sink.rows.lock().unwrap();
    assert_eq!(rows.len(), 2);
    assert!(
        rows.iter()
            .all(|(r, _)| r.callsite.contains("native-effects/src/tests.rs:")),
        "{rows:?}"
    );
    assert_eq!(rows[0].1.errno, Some(libc::EEXIST));
    assert_eq!(rows[0].1.created, None);
    assert_eq!(rows[1].1.created, Some(false));
    assert_eq!(std::fs::read(&path).unwrap(), b"before");
}
#[test]
fn guarded_options_match_std_invalid_combinations_before_effect() {
    for bits in 0..64 {
        let ordinary = tempfile::tempdir().unwrap();
        let guarded = tempfile::tempdir().unwrap();
        let p = ordinary.path().join("value");
        let q = guarded.path().join("value");
        std::fs::write(&p, b"before").unwrap();
        std::fs::write(&q, b"before").unwrap();
        let mut a = std::fs::OpenOptions::new();
        let mut b = fs::OpenOptions::new();
        a.read(bits & 1 != 0)
            .write(bits & 2 != 0)
            .append(bits & 4 != 0)
            .truncate(bits & 8 != 0)
            .create(bits & 16 != 0)
            .create_new(bits & 32 != 0);
        b.read(bits & 1 != 0)
            .write(bits & 2 != 0)
            .append(bits & 4 != 0)
            .truncate(bits & 8 != 0)
            .create(bits & 16 != 0)
            .create_new(bits & 32 != 0);
        let expected = a.open(&p);
        let (s, _, _, _) = session(guarded.path(), 10);
        {
            let _scope = observation_scope(&s).unwrap();
            let actual = b.open(&q);
            assert_eq!(
                actual.as_ref().err().map(std::io::Error::raw_os_error),
                expected.as_ref().err().map(std::io::Error::raw_os_error),
                "options {bits}"
            );
        }
        assert_eq!(
            std::fs::read(&p).unwrap(),
            std::fs::read(&q).unwrap(),
            "options {bits}"
        );
        let _ = s.finish().unwrap();
    }
}
#[test]
fn nested_cleanup_and_thread_context_cannot_lose_filesystem_authority() {
    let d = tempfile::tempdir().unwrap();
    let (s, sink, _, capture) = session(d.path(), 20);
    {
        let _scope = observation_scope(&s).unwrap();
        let _cleanup = cleanup_scope();
        let _legacy = observer_scope(capture);
        let context = ObserverContext::current();
        let path = d.path().join("thread");
        std::thread::scope(|scope| {
            scope
                .spawn(move || {
                    let _entered = context.enter();
                    fs::write(path, b"thread").unwrap();
                })
                .join()
                .unwrap();
        });
    }
    assert_eq!(
        s.finish().unwrap().completeness,
        NativeFilesystemCompleteness::Complete
    );
    assert!(
        sink.rows
            .lock()
            .unwrap()
            .iter()
            .all(|(r, _)| r.class == NativeCaptureClass::Cleanup)
    );
}
#[test]
fn same_bytes_replacement_cannot_be_adopted_or_deleted() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("known");
    let (s, _, _, _) = session(d.path(), 20);
    {
        let _scope = observation_scope(&s).unwrap();
        fs::write(&path, b"same").unwrap();
        std::fs::rename(&path, d.path().join("original")).unwrap();
        std::fs::write(&path, b"same").unwrap();
        let _cleanup = cleanup_scope();
        assert!(fs::remove_file(&path).is_err());
        assert!(path.exists());
    }
    assert_eq!(
        s.finish().unwrap().completeness,
        NativeFilesystemCompleteness::Incomplete
    );
}
#[test]
fn ignored_publication_failure_and_context_replacement_remain_incomplete() {
    let d = tempfile::tempdir().unwrap();
    let (s, sink, _, _) = session(d.path(), 10);
    sink.fail_finish.store(true, Ordering::SeqCst);
    {
        let _scope = observation_scope(&s).unwrap();
        let _ignored = fs::write(d.path().join("value"), b"value");
        let _other = observer_scope(Arc::new(Capture));
        assert!(ObserverContext::current().check_work().is_err());
    }
    assert_eq!(
        s.finish().unwrap().completeness,
        NativeFilesystemCompleteness::Incomplete
    );
}
#[test]
fn missing_nullable_and_unknown_wire_states_do_not_mean_observed_absence() {
    let actual = NativeFilesystemActual {
        result: NativeFilesystemResult::Succeeded,
        errno: None,
        written_bytes: Some(1),
        return_value: Some(1),
        created: None,
        before: vec![],
        after: vec![],
        started_ns: 1,
        ended_ns: Some(2),
    };
    let original = serde_json::to_value(actual).unwrap();
    for key in ["e", "w", "v", "new", "t"] {
        let mut value = original.clone();
        value.as_object_mut().unwrap().remove(key);
        assert!(
            serde_json::from_value::<NativeFilesystemActual>(value).is_err(),
            "{key}"
        );
    }
    let mut value = original;
    value["r"] = "future_success".into();
    assert!(serde_json::from_value::<NativeFilesystemActual>(value).is_err());
}
#[test]
fn maximum_compact_record_fits_the_fixed_reservation() {
    let identity = NativeFilesystemIdentity {
        device: u64::MAX,
        inode: u64::MAX,
        mode: u32::MAX,
        uid: u32::MAX,
        links: u64::MAX,
        bytes: u64::MAX,
    };
    let actual = NativeFilesystemActual {
        result: NativeFilesystemResult::NotAttempted,
        errno: Some(i32::MIN),
        written_bytes: Some(u64::MAX),
        return_value: Some(i64::MIN),
        created: Some(false),
        before: vec![Some(identity.clone()); 2],
        after: vec![Some(identity); 2],
        started_ns: u64::MAX,
        ended_ns: Some(u64::MAX),
    };
    let row = NativeFilesystemRecord {
        ordinal: 8191,
        control_span: Some(8191),
        operation: NativeFilesystemOperation::CreateDirectory,
        class: NativeCaptureClass::Cleanup,
        scope: NativeFilesystemScope::HistoricalRecovery,
        attempt: Some(31),
        site: 8191,
        paths: vec![2047; 2],
        requested_bytes: 512 * 1024 * 1024,
        flags: Some(i32::MIN),
        mode: Some(u32::MAX),
        actual,
    };
    let bytes = serde_json::to_vec(&row).unwrap();
    row.validate_bounds().unwrap();
    // The sink's newline is part of the immutable 768-byte reservation.
    assert!(bytes.len() < 768, "{} bytes plus newline", bytes.len());
    let _: NativeFilesystemRecord = serde_json::from_slice(&bytes).unwrap();
    for field in ["i", "z", "n", "f", "p", "q"] {
        let mut invalid = serde_json::to_value(&row).unwrap();
        invalid[field] = match field {
            "i" | "z" | "f" => serde_json::json!(8192),
            "n" => serde_json::json!(32),
            "p" => serde_json::json!([2048]),
            "q" => serde_json::json!(NATIVE_FILESYSTEM_REQUEST_BYTES + 1),
            _ => unreachable!(),
        };
        let invalid: NativeFilesystemRecord = serde_json::from_value(invalid).unwrap();
        assert!(invalid.validate_bounds().is_err(), "{field}");
    }
    let mut invalid = row;
    invalid.paths.push(0);
    assert!(invalid.validate_bounds().is_err());
}

#[test]
fn producer_refuses_excess_events_paths_and_requested_bytes_before_effects() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("unchanged");
    std::fs::write(&p, b"unchanged").unwrap();
    let (s, sink, _, _) = session(d.path(), 20);
    {
        let _scope = observation_scope(&s).unwrap();
        assert!(
            crate::reservation::Operation::begin(
                NativeFilesystemOperation::Rename,
                &[&p, &p, &p],
                0
            )
            .is_err()
        );
        let _cleanup = cleanup_scope();
        assert!(
            crate::reservation::Operation::begin(
                NativeFilesystemOperation::Write,
                &[&p],
                NATIVE_FILESYSTEM_REQUEST_BYTES + 1
            )
            .is_err()
        );
        // Test-only state injection reaches the exact counter boundary without spending
        // 8,192 unrelated syscalls. No injected state is exported as real evidence.
        s.shared.state.lock().unwrap().reserved = NATIVE_FILESYSTEM_EVENTS;
        assert!(fs::remove_file(&p).is_err());
    }
    assert_eq!(std::fs::read(&p).unwrap(), b"unchanged");
    assert!(sink.rows.lock().unwrap().is_empty());
    assert_eq!(
        s.finish().unwrap().completeness,
        NativeFilesystemCompleteness::Incomplete
    );
}

#[test]
fn producer_normal_partition_exhaustion_preserves_protected_cleanup_partition() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("owned");
    std::fs::write(&p, b"owned").unwrap();
    let (s, sink, _, _) = session(d.path(), 20);
    {
        let _scope = observation_scope(&s).unwrap();
        s.shared.state.lock().unwrap().normal_reserved = NATIVE_FILESYSTEM_NORMAL_EVENTS;
        assert!(fs::remove_file(&p).is_err());
        assert!(p.exists());
        let _cleanup = cleanup_scope();
        fs::remove_file(&p).unwrap();
        s.shared.state.lock().unwrap().cleanup_reserved = NATIVE_FILESYSTEM_CLEANUP_EVENTS;
        assert!(fs::write(d.path().join("blocked"), b"x").is_err());
        assert!(!d.path().join("blocked").exists());
    }
    assert_eq!(sink.rows.lock().unwrap().len(), 1);
    assert_eq!(
        s.finish().unwrap().completeness,
        NativeFilesystemCompleteness::Incomplete
    );
}

#[test]
fn interrupted_write_driver_retains_each_attempt_and_actual_partial_progress() {
    use std::io::Write;
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("partial");
    let (s, sink, _, _) = session(d.path(), 20);
    let mut calls = 0;
    {
        let _scope = observation_scope(&s).unwrap();
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        fs::write_all_with(&mut file, &path, b"abcde", |file, bytes| {
            calls += 1;
            if calls == 1 {
                // A deterministic transport fault, not a claim of a reproduced kernel EINTR.
                return Err(std::io::Error::from_raw_os_error(libc::EINTR));
            }
            file.write(&bytes[..bytes.len().min(2)])
        })
        .unwrap();
    }
    assert_eq!(calls, 4);
    assert_eq!(std::fs::read(&path).unwrap(), b"abcde");
    assert_eq!(
        s.finish().unwrap().completeness,
        NativeFilesystemCompleteness::Complete
    );
    let rows = sink.rows.lock().unwrap();
    let writes: Vec<_> = rows
        .iter()
        .filter(|(r, _)| r.operation == NativeFilesystemOperation::Write)
        .collect();
    assert_eq!(writes.len(), 4);
    assert_eq!(writes[0].1.result, NativeFilesystemResult::Failed);
    assert_eq!(writes[0].1.errno, Some(libc::EINTR));
    assert_eq!(writes[0].1.written_bytes, Some(0));
    assert_eq!(writes[0].1.return_value, Some(-1));
    assert_eq!(
        writes
            .iter()
            .map(|(_, a)| a.written_bytes.unwrap())
            .collect::<Vec<_>>(),
        vec![0, 2, 2, 1]
    );
}

#[test]
fn shared_failure_contract_rejects_wrong_operation_and_unknown_errno() {
    use NativeFilesystemOperation::*;
    for (operation, errno) in [
        (OpenWrite, libc::EEXIST),
        (CreateDirectory, libc::EEXIST),
        (HardLink, libc::EEXIST),
        (Unlink, libc::ENOENT),
        (RemoveDirectory, libc::ENOENT),
        (Write, libc::EINTR),
    ] {
        assert!(operation.permits_failure(Some(errno)));
        assert!(!operation.permits_failure(Some(libc::EIO)));
        assert!(!operation.permits_failure(None));
    }
    assert!(!Write.permits_failure(Some(libc::EEXIST)));
    assert!(!Rename.permits_failure(Some(libc::ENOENT)));
    assert!(!Sync.permits_failure(Some(libc::EINTR)));
}

#[test]
fn failed_write_preserves_original_errno_after_incomplete_observation() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("failed-write");
    let (s, sink, _, _) = session(d.path(), 20);
    let mut calls = 0;
    {
        let _scope = observation_scope(&s).unwrap();
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        let error = fs::write_all_with(&mut file, &path, b"unchanged", |_, _| {
            calls += 1;
            // Inject the driver result; this does not claim an actual kernel I/O failure.
            Err(std::io::Error::from_raw_os_error(libc::EIO))
        })
        .unwrap_err();
        assert_eq!(error.raw_os_error(), Some(libc::EIO));
    }
    assert_eq!(calls, 1);
    assert!(std::fs::read(&path).unwrap().is_empty());
    assert_eq!(
        s.finish().unwrap().completeness,
        NativeFilesystemCompleteness::Incomplete
    );
    let rows = sink.rows.lock().unwrap();
    let writes: Vec<_> = rows
        .iter()
        .filter(|(r, _)| r.operation == NativeFilesystemOperation::Write)
        .collect();
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0].1.result, NativeFilesystemResult::Failed);
    assert_eq!(writes[0].1.errno, Some(libc::EIO));
    assert_eq!(writes[0].1.written_bytes, Some(0));
    assert_eq!(writes[0].1.return_value, Some(-1));
}

#[test]
fn interrupted_write_cannot_retry_after_original_fence_stops() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("stopped");
    let (s, sink, fence, _) = session(d.path(), 20);
    let mut calls = 0;
    {
        let _scope = observation_scope(&s).unwrap();
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        assert!(
            fs::write_all_with(&mut file, &path, b"abc", |_, _| {
                calls += 1;
                fence.stopped.store(true, Ordering::SeqCst);
                Err(std::io::Error::from_raw_os_error(libc::EINTR))
            })
            .is_err()
        );
    }
    assert_eq!(calls, 1);
    assert!(std::fs::read(&path).unwrap().is_empty());
    assert_eq!(
        s.finish().unwrap().completeness,
        NativeFilesystemCompleteness::Incomplete
    );
    let rows = sink.rows.lock().unwrap();
    assert_eq!(
        rows.iter()
            .filter(|(r, _)| r.operation == NativeFilesystemOperation::Write)
            .count(),
        1
    );
    assert_eq!(rows.last().unwrap().1.errno, Some(libc::EINTR));
}

#[test]
fn removed_name_cannot_be_reused_by_protected_cleanup() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("stdout.jsonl");
    let (s, _, _, _) = session(d.path(), 20);
    {
        let _scope = observation_scope(&s).unwrap();
        fs::write(&path, b"owned").unwrap();
        fs::remove_file(&path).unwrap();
        std::fs::write(&path, b"new occupant").unwrap();
        let _cleanup = cleanup_scope();
        assert!(fs::remove_file(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"new occupant");
    }
    assert_eq!(
        s.finish().unwrap().completeness,
        NativeFilesystemCompleteness::Incomplete
    );
}

#[test]
fn original_stop_allows_owned_cleanup_only_until_its_original_cutoff() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("owned");
    let q = d.path().join("retained");
    let (s, _, fence, _) = session(d.path(), 20);
    {
        let _scope = observation_scope(&s).unwrap();
        fs::write(&p, b"owned").unwrap();
        fs::write(&q, b"retained").unwrap();
        fence.stopped.store(true, Ordering::SeqCst);
        assert!(fs::remove_file(&p).is_err());
        let _cleanup = cleanup_scope();
        fs::remove_file(&p).unwrap();
        fence.cleanup_stopped.store(true, Ordering::SeqCst);
        assert!(fs::remove_file(&q).is_err());
        assert_eq!(std::fs::read(&q).unwrap(), b"retained");
    }
    assert_eq!(
        s.finish().unwrap().completeness,
        NativeFilesystemCompleteness::Incomplete
    );
}

#[test]
fn anchored_syscall_name_must_match_its_approved_metadata_path() {
    let d = tempfile::tempdir().unwrap();
    let parent = std::fs::File::open(d.path()).unwrap();
    let p = d.path().join("approved");
    let foreign = d.path().join("foreign");
    std::fs::write(&foreign, b"retained").unwrap();
    let (s, sink, _, _) = session(d.path(), 20);
    {
        let _scope = observation_scope(&s).unwrap();
        assert!(fs::mkdir_at(&parent, &p, c"foreign-directory", 0o700).is_err());
        let _cleanup = cleanup_scope();
        assert!(fs::unlink_at(&parent, &p, c"foreign").is_err());
    }
    assert!(!d.path().join("foreign-directory").exists());
    assert_eq!(std::fs::read(&foreign).unwrap(), b"retained");
    assert!(sink.rows.lock().unwrap().is_empty());
    assert_eq!(
        s.finish().unwrap().completeness,
        NativeFilesystemCompleteness::Incomplete
    );
}

#[test]
fn wrong_parent_fd_with_same_leaf_cannot_mutate_either_owned_directory() {
    let d = tempfile::tempdir().unwrap();
    let a = d.path().join("approved");
    let b = d.path().join("other");
    std::fs::create_dir(&a).unwrap();
    std::fs::create_dir(&b).unwrap();
    std::fs::write(b.join("leaf"), b"other-owned-sentinel").unwrap();
    let other = std::fs::File::open(&b).unwrap();
    let (s, sink, _, _) = session(d.path(), 20);
    {
        let _scope = observation_scope(&s).unwrap();
        assert!(fs::mkdir_at(&other, &a.join("new"), c"new", 0o700).is_err());
        let _cleanup = cleanup_scope();
        assert!(fs::unlink_at(&other, &a.join("leaf"), c"leaf").is_err());
    }
    assert_eq!(std::fs::read_dir(&a).unwrap().count(), 0);
    assert_eq!(std::fs::read_dir(&b).unwrap().count(), 1);
    assert_eq!(
        std::fs::read(b.join("leaf")).unwrap(),
        b"other-owned-sentinel"
    );
    let rows = sink.rows.lock().unwrap();
    assert_eq!(rows.len(), 2);
    assert!(
        rows.iter()
            .all(|(_, a)| a.result == NativeFilesystemResult::NotAttempted
                && a.errno.is_none()
                && a.return_value.is_none())
    );
    drop(rows);
    assert_eq!(
        s.finish().unwrap().completeness,
        NativeFilesystemCompleteness::Incomplete
    );
}
