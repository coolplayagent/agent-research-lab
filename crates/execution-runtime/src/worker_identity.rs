//! Linux process start identity used to reject recycled PIDs.
use anyhow::{Context, Result};
use native_effects::fs;

pub(super) fn process_start(pid: u32) -> Result<String> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat"))?;
    stat.rsplit_once(") ")
        .context("invalid proc stat")?
        .1
        .split_whitespace()
        .nth(19)
        .map(str::to_owned)
        .context("missing process start time")
}

#[cfg(test)]
#[path = "worker_identity_tests.rs"]
mod tests;
