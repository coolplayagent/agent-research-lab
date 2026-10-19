//! Bounded JSON persistence and single-owner state locks.
#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(unix)]
pub use unix::*;
#[cfg(windows)]
pub use windows::*;

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
