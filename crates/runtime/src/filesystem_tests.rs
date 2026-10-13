//! Real parent IO and harmless transport through the optional production entry.
use super::*;
use contracts::*;
use native_effects::{FilesystemObserver, FilesystemReservation, ObservationFence};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

#[derive(Default)]
struct Recorder {
    rows: Mutex<Vec<(NativeFilesystemRequest, NativeFilesystemActual)>>,
    children: Mutex<Vec<process::CapturedControl>>,
    normal: AtomicU64,
    fail: AtomicBool,
}
struct Row {
    recorder: Arc<Recorder>,
    request: NativeFilesystemRequest,
}
impl FilesystemReservation for Row {
    fn finish(self: Box<Self>, actual: &NativeFilesystemActual) -> Result<()> {
        self.recorder
            .rows
            .lock()
            .unwrap()
            .push((self.request, actual.clone()));
        ensure!(
            !self.recorder.fail.load(Ordering::SeqCst),
            "injected publication failure"
        );
        Ok(())
    }
}
struct FsSink(Arc<Recorder>);
impl FilesystemObserver for FsSink {
    fn reserve(&self, request: &NativeFilesystemRequest) -> Result<Box<dyn FilesystemReservation>> {
        if request.class == NativeCaptureClass::Normal {
            self.0
                .normal
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |v| v.checked_sub(1))
                .map_err(|_| anyhow::anyhow!("test normal capacity exhausted"))?;
        }
        Ok(Box::new(Row {
            recorder: self.0.clone(),
            request: request.clone(),
        }))
    }
}
struct ChildReservation(Arc<Recorder>);
impl process::CaptureReservation for ChildReservation {
    fn control_ordinal(&self) -> Option<u16> {
        Some(7)
    }
    fn check_work(&self) -> Result<()> {
        Ok(())
    }
    fn check_cleanup(&self) -> Result<()> {
        Ok(())
    }
    fn retained_stream_limit(&self) -> u64 {
        65535
    }
    fn finish(&mut self, actual: &process::CapturedControl) -> Result<()> {
        self.0.children.lock().unwrap().push(actual.clone());
        Ok(())
    }
}
impl process::CaptureObserver for Recorder {
    fn reserve(&self, _: &process::CaptureRequest) -> Result<Box<dyn process::CaptureReservation>> {
        bail!("use the fixture's exact shared capture recorder")
    }
}
struct CaptureSink(Arc<Recorder>);
impl process::CaptureObserver for CaptureSink {
    fn reserve(&self, _: &process::CaptureRequest) -> Result<Box<dyn process::CaptureReservation>> {
        Ok(Box::new(ChildReservation(self.0.clone())))
    }
}
struct Fence(std::time::Instant);
impl ObservationFence for Fence {
    fn now_ns(&self) -> Result<u64> {
        Ok(self.0.elapsed().as_nanos().try_into()?)
    }
    fn check_work(&self) -> Result<()> {
        ensure!(
            self.0.elapsed() < Duration::from_secs(10),
            "original fixture work cutoff"
        );
        Ok(())
    }
    fn check_cleanup(&self) -> Result<()> {
        ensure!(
            self.0.elapsed() < Duration::from_secs(12),
            "original fixture cleanup cutoff"
        );
        Ok(())
    }
}
fn session(
    root: &Path,
    normal: u64,
    object_limit: u64,
) -> (native_effects::NativeObservationSession, Arc<Recorder>) {
    let r = Arc::new(Recorder::default());
    r.normal.store(normal, Ordering::SeqCst);
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
    let s = native_effects::NativeObservationSession::new(
        NativeFilesystemBinding {
            schema_version: 1,
            session_sha256: "1".repeat(64),
            source_sha256: "2".repeat(64),
            config_sha256: "3".repeat(64),
            allowlist_sha256: "4".repeat(64),
            pid: std::process::id(),
            process_start_ticks: start,
        },
        native_effects::FilesystemPolicy {
            roots: vec![native_effects::FilesystemRoot {
                id: 0,
                path: root.into(),
                category: NativeFilesystemCategory::Json,
                max_object_bytes: object_limit,
            }],
            attempts: vec![],
            excluded: vec![],
            max_concurrent_operations: 8,
        },
        Arc::new(CaptureSink(r.clone())),
        Arc::new(FsSink(r.clone())),
        Arc::new(Fence(std::time::Instant::now())),
    )
    .unwrap();
    (s, r)
}

#[test]
fn workflow_protocol_publication_retains_transient_hardlink_and_unlink_facts() {
    let root = tempfile::tempdir().unwrap();
    let (session, recorder) = session(&std::env::temp_dir(), 128, 16 * 1024 * 1024);
    {
        let _scope = native_effects::observation_scope(&session).unwrap();
        // The real adapter persists its request, then invokes only this harmless failing transport.
        // No Workflow owner or database is opened by /bin/false.
        let workflow = workflow::Workflow::new(
            Path::new("/bin/false"),
            root.path().join("never-opened.sqlite"),
            root.path(),
        );
        assert!(workflow.claim(&json!({"synthetic_lease": true})).is_err());
    }
    assert!(!root.path().join("never-opened.sqlite").exists());
    let files: Vec<_> = std::fs::read_dir(root.path().join("protocol"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        serde_json::from_slice::<Value>(&std::fs::read(&files[0]).unwrap()).unwrap(),
        json!({"synthetic_lease": true})
    );
    assert_eq!(
        session.finish().unwrap().completeness,
        NativeFilesystemCompleteness::Complete
    );
    let rows = recorder.rows.lock().unwrap();
    let (_, linked) = rows
        .iter()
        .find(|(request, _)| request.operation == NativeFilesystemOperation::HardLink)
        .unwrap();
    assert_eq!(linked.result, NativeFilesystemResult::Succeeded);
    assert_eq!(
        linked.after[0].as_ref().unwrap().inode,
        linked.after[1].as_ref().unwrap().inode
    );
    assert!(rows.iter().any(|(request, actual)| {
        request.operation == NativeFilesystemOperation::Unlink
            && request.paths[0].relative.contains("/protocol/.pending-")
            && actual.result == NativeFilesystemResult::Succeeded
            && actual.after == [None]
    }));
    let children = recorder.children.lock().unwrap();
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].actual_exit, Some(1));
    assert!(children[0].reaped);
}

#[test]
fn storage_pending_publication_and_ignored_unlock_failure_are_observed() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("state.json");
    let (s, r) = session(d.path(), 64, 4096);
    {
        let _scope = native_effects::observation_scope(&s).unwrap();
        storage::write(&p, &json!({"current":1})).unwrap();
        assert_eq!(storage::read::<Value>(&p).unwrap(), json!({"current":1}));
        assert_eq!(std::fs::read_dir(d.path()).unwrap().count(), 1);
        let lock = storage::lock(&d.path().join("controller.lock")).unwrap();
        r.fail.store(true, Ordering::SeqCst);
        drop(lock);
    }
    assert_eq!(
        s.finish().unwrap().completeness,
        NativeFilesystemCompleteness::Incomplete
    );
    let rows = r.rows.lock().unwrap();
    let rename = rows
        .iter()
        .find(|(q, _)| q.operation == NativeFilesystemOperation::Rename)
        .unwrap();
    assert!(rename.0.paths[0].relative.contains("pending-"));
    assert_eq!(rename.0.paths[1].relative, "state.json");
    assert!(rename.1.after[0].is_none());
    assert_eq!(
        rename.1.before[0].as_ref().unwrap().inode,
        rename.1.after[1].as_ref().unwrap().inode
    );
    assert!(
        rows.iter()
            .any(|(q, a)| q.operation == NativeFilesystemOperation::Unlock
                && q.class == NativeCaptureClass::Cleanup
                && a.result == NativeFilesystemResult::Succeeded)
    );
}

#[test]
fn bounded_storage_serialization_rejects_before_any_pending_creation() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("state.json");
    std::fs::write(&p, b"old").unwrap();
    let (s, r) = session(d.path(), 64, 64);
    {
        let _scope = native_effects::observation_scope(&s).unwrap();
        assert!(storage::write(&p, &json!({"large":"x".repeat(256)})).is_err());
    }
    assert_eq!(std::fs::read(&p).unwrap(), b"old");
    assert_eq!(std::fs::read_dir(d.path()).unwrap().count(), 1);
    assert!(r.rows.lock().unwrap().is_empty());
    assert_eq!(
        s.finish().unwrap().completeness,
        NativeFilesystemCompleteness::Incomplete
    );
}

#[test]
fn actual_capture_retains_span_through_normal_or_protected_scratch_teardown() {
    let d = tempfile::tempdir().unwrap();
    for capacity in [64, 3, 4] {
        let (s, r) = session(&std::env::temp_dir(), capacity, 16 * 1024 * 1024);
        {
            let _scope = native_effects::observation_scope(&s).unwrap();
            let result = process::capture(
                "/bin/sleep",
                &["0.1".into()],
                d.path(),
                Duration::from_secs(2),
            );
            assert_eq!(result.is_ok(), capacity == 64);
        }
        let report = s.finish().unwrap();
        assert_eq!(
            report.completeness,
            if capacity == 64 {
                NativeFilesystemCompleteness::Complete
            } else {
                NativeFilesystemCompleteness::Incomplete
            }
        );
        let children = r.children.lock().unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].actual_exit, Some(0));
        assert!(children[0].reaped);
        let rows = r.rows.lock().unwrap();
        assert!(rows.iter().all(|(q, _)| q.control_span == Some(7)
            && q.scope == NativeFilesystemScope::CaptureTemporary));
        let removed = rows
            .iter()
            .filter(|(q, _)| {
                matches!(
                    q.operation,
                    NativeFilesystemOperation::Unlink | NativeFilesystemOperation::RemoveDirectory
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(removed.len(), 3);
        assert_eq!(
            removed
                .iter()
                .filter(|(q, _)| q.class == NativeCaptureClass::Normal)
                .count(),
            if capacity == 64 {
                3
            } else {
                capacity as usize - 3
            }
        );
        for (q, _) in removed {
            assert!(!std::env::temp_dir().join(&q.paths[0].relative).exists());
        }
    }
}

#[test]
fn dual_observer_rejects_before_controller_lock_or_owner_effects() {
    let (_temp, mut c, _, _) = tests::completion_fixture(false, json!([]));
    for name in [
        "workflow-cli",
        "relay-knowledge",
        "into-markdown",
        "qualitygate-cli",
        "computer-use-cli",
        "relay-memory",
        "repo-sandbox",
    ] {
        c.tools.entry(name.into()).or_insert_with(|| config::Tool {
            binary: "/bin/false".into(),
            repository: c.superpod.clone(),
            probe: vec![],
        });
    }
    c.validate().unwrap();
    let (s, r) = session(&c.state_dir, 0, 16 * 1024 * 1024);
    struct Hook;
    impl admission::ScopedAdmissionHook for Hook {
        fn register_cleanup_domain(&mut self, _: &Config, _: &Path, _: u64) -> Result<()> {
            panic!("lock admission must fail first")
        }
        fn prepare(&mut self, _: &Config, _: &[Job]) -> Result<()> {
            panic!("no owner admission")
        }
        fn take_acquire_preconditions(
            &mut self,
            _: &Config,
            _: &Job,
        ) -> Result<AcquirePreconditionsV1> {
            panic!("no lease")
        }
        fn before_model_spawn(&mut self, _: &Config, _: &Job) -> Result<()> {
            panic!("no model")
        }
        fn should_stop(&self) -> bool {
            false
        }
        fn revoke(&mut self, _: &str) {}
    }
    let result =
        run_scoped_with_observations(&c, 1, &c.state_dir.join("unused-scope.json"), &mut Hook, s);
    assert!(format!("{:#}", result.result.unwrap_err()).contains("test normal capacity exhausted"));
    assert_eq!(
        result.filesystem.unwrap().completeness,
        NativeFilesystemCompleteness::Incomplete
    );
    assert!(!c.state_dir.join("controller.lock").exists());
    assert!(r.children.lock().unwrap().is_empty());
}
