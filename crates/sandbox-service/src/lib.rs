//! Disposable execution with host-verified binding challenges.
// Native plugin catalogs can exceed 16 MiB independently of task output.
// Keep runtime files bounded while retaining the smaller protocol/log limits.
const MAX_RUNTIME_FILE: u64 = 64 * 1024 * 1024;
mod challenge;
mod diagnostics;
#[cfg(windows)]
mod profiles;
pub fn recover(state: &std::path::Path) -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        profiles::cleanup(state)?;
    }
    #[cfg(unix)]
    {
        let _ = state;
    }
    Ok(())
}
#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;
#[cfg(unix)]
pub use unix::{probe, run, verify};
#[cfg(windows)]
pub use windows::{probe, probe_child, run, verify};
