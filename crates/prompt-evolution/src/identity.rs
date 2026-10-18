//! Stable serialized payload identities and required-text validation.
use anyhow::{Result, ensure};
use serde::Serialize;
use sha2::{Digest, Sha256};

pub(super) fn digest(value: &impl Serialize) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(value)?)))
}

pub(super) fn nonempty(value: &str, field: &str) -> Result<()> {
    ensure!(!value.trim().is_empty(), "{field} must not be empty");
    Ok(())
}
