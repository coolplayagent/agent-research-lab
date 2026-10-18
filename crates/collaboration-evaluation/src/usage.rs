//! Bounded worker-writable usage diagnostics; never authoritative billing or approval.
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{fs, path::Path};

pub(super) fn usage_trace(path: &Path) -> Value {
    // This is a diagnostic worker-writable trace, not an independent billing record.
    let read = || -> Result<Value> {
        use std::{io::Read, os::unix::fs::OpenOptionsExt};
        let mut file = fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(path)?;
        let metadata = file.metadata()?;
        ensure!(
            metadata.is_file() && metadata.len() <= 16 * 1024 * 1024,
            "trace must be bounded regular file"
        );
        let mut bytes = Vec::new();
        file.by_ref()
            .take(16 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        ensure!(bytes.len() <= 16 * 1024 * 1024, "trace grew beyond limit");
        let mut input = 0_u64;
        let mut output = 0_u64;
        let mut turns = 0;
        for line in bytes.split(|b| *b == b'\n').filter(|l| !l.is_empty()) {
            let value: Value = serde_json::from_slice(line)?;
            if value["type"] == "turn.completed" {
                input = input
                    .checked_add(
                        value["usage"]["input_tokens"]
                            .as_u64()
                            .context("missing token usage")?,
                    )
                    .context("token overflow")?;
                output = output
                    .checked_add(
                        value["usage"]["output_tokens"]
                            .as_u64()
                            .context("missing token usage")?,
                    )
                    .context("token overflow")?;
                turns += 1;
            }
        }
        Ok(
            json!({"trace_sha256":storage::digest(&bytes),"completed_turns":turns,"input_tokens":if turns>0{Some(input)}else{None},"output_tokens":if turns>0{Some(output)}else{None}}),
        )
    };
    match read() {
        Ok(v) => json!({"trust":"worker_writable_diagnostic_only","observed":v}),
        Err(e) => {
            json!({"trust":"worker_writable_diagnostic_only","observed":null,"error":e.to_string()})
        }
    }
}

#[cfg(test)]
#[path = "usage_tests.rs"]
mod tests;
