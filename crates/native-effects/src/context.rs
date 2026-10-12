use crate::{CaptureObserver, FilesystemPolicy};
use anyhow::{Result, ensure};
use contracts::*;
use std::{
    cell::{Cell, RefCell},
    marker::PhantomData,
    rc::Rc,
    sync::{Arc, Mutex},
};

pub trait ObservationFence: Send + Sync {
    /// The original selected clock domain; no new duration or stop handler.
    fn now_ns(&self) -> Result<u64>;
    fn check_work(&self) -> Result<()>;
    fn check_cleanup(&self) -> Result<()>;
    /// Check the original final cleanup cutoff, live stop authority and observer health.
    /// Implementations that distinguish natural work expiry may override this conservative default.
    fn check_finalization(&self) -> Result<()> {
        self.check_work()?;
        self.check_cleanup()
    }
}
pub trait FilesystemReservation: Send {
    /// Called once with real OS facts, including failure/unknown; never request bytes as actual.
    fn finish(self: Box<Self>, actual: &NativeFilesystemActual) -> Result<()>;
}
pub trait FilesystemObserver: Send + Sync {
    /// Reserve bounded metadata and protected cleanup space before any effect.
    fn reserve(&self, request: &NativeFilesystemRequest) -> Result<Box<dyn FilesystemReservation>>;
}

#[derive(Default)]
pub(crate) struct State {
    pub fault: Option<String>,
    pub reserved: u64,
    pub normal_reserved: u64,
    pub cleanup_reserved: u64,
    pub finalized: u64,
    pub outstanding: u32,
    pub entered: u32,
    pub closed: bool,
    pub known:
        std::collections::BTreeMap<std::path::PathBuf, Option<contracts::NativeFilesystemIdentity>>,
}
pub(crate) struct Shared {
    pub binding: NativeFilesystemBinding,
    pub policy: FilesystemPolicy,
    pub observer: Arc<dyn FilesystemObserver>,
    pub fence: Arc<dyn ObservationFence>,
    pub state: Mutex<State>,
}
impl Shared {
    pub fn fault(&self, code: &str) {
        if let Ok(mut s) = self.state.lock()
            && s.fault.is_none()
        {
            s.fault = Some(code.chars().take(128).collect());
        }
    }
    pub fn check(&self, class: NativeCaptureClass) -> Result<()> {
        let check = (|| {
            {
                let s = self
                    .state
                    .lock()
                    .map_err(|_| anyhow::anyhow!("filesystem state poisoned"))?;
                ensure!(!s.closed, "filesystem session closed");
                ensure!(
                    class == NativeCaptureClass::Cleanup || s.fault.is_none(),
                    "filesystem admission revoked"
                );
            }
            if class == NativeCaptureClass::Cleanup {
                self.fence.check_cleanup()
            } else {
                self.fence.check_work()
            }
        })();
        if check.is_err() {
            self.fault("filesystem_fence_failed");
        }
        check
    }
}

/// Live, source-owned observation authority. No Clone or Deserialize implementation.
pub struct NativeObservationSession {
    pub(crate) shared: Arc<Shared>,
    capture: Arc<dyn CaptureObserver>,
}
impl NativeObservationSession {
    pub fn new(
        binding: NativeFilesystemBinding,
        policy: FilesystemPolicy,
        capture: Arc<dyn CaptureObserver>,
        observer: Arc<dyn FilesystemObserver>,
        fence: Arc<dyn ObservationFence>,
    ) -> Result<Self> {
        policy.validate()?;
        let stat = std::fs::read_to_string("/proc/self/stat")?;
        let start: u64 = stat
            .rsplit_once(") ")
            .ok_or_else(|| anyhow::anyhow!("filesystem process identity"))?
            .1
            .split_whitespace()
            .nth(19)
            .ok_or_else(|| anyhow::anyhow!("filesystem process start"))?
            .parse()?;
        ensure!(
            start == binding.process_start_ticks,
            "filesystem process start differs"
        );
        let open_fds = std::fs::read_dir("/proc/self/fd")?
            .take(4097)
            .collect::<std::io::Result<Vec<_>>>()?
            .len();
        ensure!(open_fds <= 4096, "filesystem baseline FD census bound");
        let required = (open_fds as u64)
            .checked_add(
                u64::from(policy.max_concurrent_operations)
                    .checked_mul(4)
                    .ok_or_else(|| anyhow::anyhow!("filesystem FD arithmetic"))?,
            )
            .and_then(|v| v.checked_add(16))
            .ok_or_else(|| anyhow::anyhow!("filesystem FD arithmetic"))?;
        let mut limit = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        ensure!(
            unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit) } == 0
                && limit.rlim_cur >= required,
            "filesystem FD allowance unavailable"
        );
        ensure!(
            binding.schema_version == 1
                && binding.pid == std::process::id()
                && binding.process_start_ticks > 0,
            "filesystem session identity"
        );
        for v in [
            &binding.session_sha256,
            &binding.source_sha256,
            &binding.config_sha256,
            &binding.allowlist_sha256,
        ] {
            ensure!(
                v.len() == 64
                    && v.bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
                "filesystem binding digest"
            );
        }
        fence.check_work()?;
        Ok(Self {
            shared: Arc::new(Shared {
                binding,
                policy,
                observer,
                fence,
                state: Mutex::new(State::default()),
            }),
            capture,
        })
    }
    pub fn check_work(&self) -> Result<()> {
        self.shared.check(NativeCaptureClass::Normal)
    }
    /// Consumed only after the runtime's guards and existing writer threads are joined.
    pub fn finish(self) -> Result<NativeFilesystemReport> {
        if self.shared.fence.check_finalization().is_err() {
            self.shared.fault("filesystem_final_fence_failed");
        }
        let mut s = self
            .shared
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("filesystem state poisoned"))?;
        ensure!(!s.closed, "filesystem session finalized twice");
        s.closed = true;
        if s.outstanding != 0 || s.entered != 0 || s.reserved != s.finalized {
            s.fault
                .get_or_insert("filesystem_operations_unsettled".into());
        }
        Ok(NativeFilesystemReport {
            schema_version: 1,
            binding: self.shared.binding.clone(),
            completeness: if s.fault.is_none() {
                NativeFilesystemCompleteness::Complete
            } else {
                NativeFilesystemCompleteness::Incomplete
            },
            reserved: s.reserved,
            finalized: s.finalized,
            outstanding: s.outstanding,
            fault: s.fault.clone(),
        })
    }
}

#[derive(Clone)]
pub struct ObserverContext {
    observer: Option<Arc<dyn CaptureObserver>>,
    class: NativeCaptureClass,
    pub(crate) filesystem: Option<Arc<Shared>>,
    pub(crate) scope: NativeFilesystemScope,
    pub(crate) attempt: Option<NativeFilesystemAttempt>,
    pub(crate) control_span: Option<u16>,
}
thread_local! {
    static CURRENT: RefCell<ObserverContext> = RefCell::new(ObserverContext { observer: None, class: NativeCaptureClass::Normal, filesystem: None, scope: NativeFilesystemScope::Controller, attempt: None, control_span: None });
    pub(crate) static CALLBACK: Cell<bool> = const { Cell::new(false) };
}
pub struct ObserverGuard(ObserverContext, Option<Arc<Shared>>, PhantomData<Rc<()>>);
impl Drop for ObserverGuard {
    fn drop(&mut self) {
        if let Some(shared) = &self.1
            && let Ok(mut s) = shared.state.lock()
        {
            s.entered = s.entered.checked_sub(1).unwrap_or_else(|| {
                s.fault.get_or_insert("filesystem_guard_underflow".into());
                0
            });
        }
        CURRENT.with(|current| *current.borrow_mut() = self.0.clone());
    }
}
impl ObserverContext {
    pub fn current() -> Self {
        CURRENT.with(|current| current.borrow().clone())
    }
    pub fn enter(&self) -> ObserverGuard {
        let prior = Self::current();
        let mut next = self.clone();
        if let Some(active) = &prior.filesystem {
            let same = next
                .filesystem
                .as_ref()
                .is_some_and(|p| Arc::ptr_eq(active, p));
            let same_capture = prior
                .observer
                .as_ref()
                .zip(next.observer.as_ref())
                .is_some_and(|(a, b)| Arc::ptr_eq(a, b));
            if !same || !same_capture {
                active.fault("filesystem_context_replacement");
                next = prior.clone();
            }
            if prior.class == NativeCaptureClass::Cleanup {
                next.class = NativeCaptureClass::Cleanup;
            }
        }
        if let Some(s) = &next.filesystem
            && let Ok(mut state) = s.state.lock()
        {
            if state.closed {
                state
                    .fault
                    .get_or_insert("filesystem_closed_context_entry".into());
            }
            state.entered = state.entered.checked_add(1).unwrap_or_else(|| {
                state
                    .fault
                    .get_or_insert("filesystem_guard_overflow".into());
                u32::MAX
            });
        }
        CURRENT.with(|c| *c.borrow_mut() = next.clone());
        ObserverGuard(prior, next.filesystem, PhantomData)
    }
    pub fn observer(&self) -> Option<&Arc<dyn CaptureObserver>> {
        self.observer.as_ref()
    }
    pub fn class(&self) -> NativeCaptureClass {
        self.class
    }
    pub fn check_work(&self) -> Result<()> {
        self.filesystem
            .as_ref()
            .map_or(Ok(()), |s| s.check(self.class))
    }
    pub fn normal_revoked(&self) -> bool {
        self.filesystem.as_ref().is_some_and(|s| {
            s.state
                .lock()
                .map_or(true, |state| state.fault.is_some() || state.closed)
        })
    }
}
pub fn observation_scope(session: &NativeObservationSession) -> Result<ObserverGuard> {
    session.check_work()?;
    let context = ObserverContext {
        observer: Some(session.capture.clone()),
        class: NativeCaptureClass::Normal,
        filesystem: Some(session.shared.clone()),
        scope: NativeFilesystemScope::Controller,
        attempt: None,
        control_span: None,
    };
    let guard = context.enter();
    context.check_work()?;
    Ok(guard)
}
pub fn observer_scope(observer: Arc<dyn CaptureObserver>) -> ObserverGuard {
    let mut next = ObserverContext::current();
    if next.filesystem.is_none() {
        next.class = NativeCaptureClass::Normal;
    }
    next.observer = Some(observer);
    next.enter()
}
pub fn cleanup_scope() -> ObserverGuard {
    let mut next = ObserverContext::current();
    next.class = NativeCaptureClass::Cleanup;
    next.enter()
}
pub fn attempt_scope(attempt: NativeFilesystemAttempt) -> Result<ObserverGuard> {
    let mut next = ObserverContext::current();
    if let Some(s) = &next.filesystem {
        if !s.policy.attempts.contains(&attempt) {
            s.fault("filesystem_attempt_mismatch");
            anyhow::bail!("filesystem attempt not host-confirmed");
        }
        s.check(next.class)?;
    }
    next.scope = NativeFilesystemScope::SelectedAttempt;
    next.attempt = Some(attempt);
    Ok(next.enter())
}

pub fn capture_scope() -> ObserverGuard {
    let mut next = ObserverContext::current();
    next.scope = NativeFilesystemScope::CaptureTemporary;
    next.enter()
}
impl Drop for NativeObservationSession {
    fn drop(&mut self) {
        if let Ok(mut s) = self.shared.state.lock()
            && !s.closed
        {
            s.fault.get_or_insert("filesystem_session_abandoned".into());
            s.closed = true;
        }
    }
}

pub fn control_scope(ordinal: Option<u16>) -> Result<ObserverGuard> {
    let mut next = ObserverContext::current();
    if let Some(shared) = &next.filesystem {
        if !ordinal.is_some_and(|v| v < 8192) {
            shared.fault("filesystem_control_span_missing");
            anyhow::bail!("guarded capture requires its actual control ordinal");
        }
        shared.check(next.class)?;
    }
    next.control_span = ordinal;
    Ok(next.enter())
}
