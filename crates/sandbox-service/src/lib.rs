//! Disposable execution with host-verified binding challenges.
mod challenge;
#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;
#[cfg(unix)]
pub use unix::{probe, run, verify};
#[cfg(windows)]
pub use windows::{probe, run, verify};
