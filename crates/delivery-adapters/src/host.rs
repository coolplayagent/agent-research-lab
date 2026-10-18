//! Bounded command execution and local operation identifiers.
use anyhow::{Result, ensure};
use std::{
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const TIMEOUT: Duration = Duration::from_secs(120);

pub fn unique_id() -> String {
    format!(
        "{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    )
}

pub fn capture_text(program: &str, args: &[String], cwd: &Path) -> Result<String> {
    let output = process::capture(program, args, cwd, TIMEOUT)?;
    ensure!(
        output.status.success(),
        "{program} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}
