//! An opt-in observer of actual native capture children, separate from O RawWait.
use anyhow::Result;
use contracts::{NativeCaptureClass, NativeCaptureState};
use std::{path::PathBuf, time::Duration};

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
    /// Source-owned control ledger ordinal; required only by the dual-observer entry.
    fn control_ordinal(&self) -> Option<u16> {
        None
    }
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
