//! Disposable execution with host-verified binding challenges.
mod challenge;
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
pub use windows::{probe, run, verify};
