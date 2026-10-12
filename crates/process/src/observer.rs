//! An opt-in observer of actual native capture children, separate from O RawWait.
use anyhow::Result;
use contracts::{NativeCaptureClass, NativeCaptureState};
use std::{cell::RefCell, path::PathBuf, sync::Arc, time::Duration};

#[derive(Clone, Debug)]
pub struct CaptureRequest {
    pub program: String,
    pub argv: Vec<String>,
    pub cwd: PathBuf,
    pub timeout: Duration,
    pub class: NativeCaptureClass,
}

#[derive(Clone, Debug, Default)]
pub struct CapturedBytes {
    pub retained: Vec<u8>,
    pub observed_file_bytes: Option<u64>,
    pub complete: bool,
    pub truncated: bool,
    pub error: Option<String>,
}

#[derive(Clone, Debug)]
pub struct CapturedControl {
    pub state: NativeCaptureState,
    pub pid: Option<u32>,
    pub process_start_ticks: Option<u64>,
    pub observed_executable: Option<PathBuf>,
    pub actual_exit: Option<i32>,
    pub reaped: bool,
    pub stdout: CapturedBytes,
    pub stderr: CapturedBytes,
    pub failure: Option<String>,
    pub cleanup_error: Option<String>,
}

pub trait CaptureReservation: Send {
    /// Check the original selected clock and stop authority; never reset a timer.
    fn check_work(&self) -> Result<()>;
    /// Cleanup has its own original cutoff and remains available after stop.
    fn check_cleanup(&self) -> Result<()>;
    fn retained_stream_limit(&self) -> u64;
    /// The caller supplies only its actual owned child identity observations.
    fn child_spawned(&mut self, _pid: u32, _start_ticks: Option<u64>) -> Result<()> {
        Ok(())
    }
    /// Called before the temporary capture files are removed, even after failure.
    fn finish(&mut self, observed: &CapturedControl) -> Result<()>;
}

pub trait CaptureObserver: Send + Sync {
    /// Reserve an ordinal, all separate evidence slots and capacity before spawn.
    fn reserve(&self, request: &CaptureRequest) -> Result<Box<dyn CaptureReservation>>;
}

#[derive(Clone)]
pub struct ObserverContext {
    observer: Option<Arc<dyn CaptureObserver>>,
    class: NativeCaptureClass,
}
thread_local! {
    static CURRENT: RefCell<ObserverContext> = RefCell::new(ObserverContext {
        observer: None,
        class: NativeCaptureClass::Normal,
    });
}
pub struct ObserverGuard(ObserverContext, std::marker::PhantomData<std::rc::Rc<()>>);
impl Drop for ObserverGuard {
    fn drop(&mut self) {
        CURRENT.with(|current| *current.borrow_mut() = self.0.clone());
    }
}
impl ObserverContext {
    pub fn current() -> Self {
        CURRENT.with(|current| current.borrow().clone())
    }
    pub fn enter(&self) -> ObserverGuard {
        let prior = CURRENT.with(|current| current.replace(self.clone()));
        ObserverGuard(prior, std::marker::PhantomData)
    }
    pub(crate) fn observer(&self) -> Option<&Arc<dyn CaptureObserver>> {
        self.observer.as_ref()
    }
    pub(crate) fn class(&self) -> NativeCaptureClass {
        self.class
    }
}
pub fn observer_scope(observer: Arc<dyn CaptureObserver>) -> ObserverGuard {
    let context = ObserverContext {
        observer: Some(observer),
        class: NativeCaptureClass::Normal,
    };
    context.enter()
}
pub fn cleanup_scope() -> ObserverGuard {
    let mut context = ObserverContext::current();
    context.class = NativeCaptureClass::Cleanup;
    context.enter()
}
