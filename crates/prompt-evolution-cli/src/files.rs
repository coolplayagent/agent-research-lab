//! Strict recursive JSON decoding and durable optional output writes.
use anyhow::{Context, Result};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::{fs, path::Path};

pub(super) fn read_strict<T: DeserializeOwned + Serialize>(path: &Path) -> Result<T> {
    let value: Value = serde_json::from_slice(
        &fs::read(path).with_context(|| format!("cannot read {}", path.display()))?,
    )?;
    let decoded: T = serde_json::from_value(value.clone())
        .with_context(|| format!("invalid request or evidence: {}", path.display()))?;
    reject_extra_fields(&value, &serde_json::to_value(&decoded)?, "$")?;
    Ok(decoded)
}

// Reject unknown nested keys before deserialization can silently discard them.
pub(super) fn reject_extra_fields(input: &Value, typed: &Value, location: &str) -> Result<()> {
    match (input, typed) {
        (Value::Object(input), Value::Object(typed)) => {
            for (key, value) in input {
                let next = format!("{location}.{key}");
                let expected = typed
                    .get(key)
                    .with_context(|| format!("unknown field {next}"))?;
                reject_extra_fields(value, expected, &next)?;
            }
        }
        (Value::Array(input), Value::Array(typed)) => {
            for (index, (input, typed)) in input.iter().zip(typed).enumerate() {
                reject_extra_fields(input, typed, &format!("{location}[{index}]"))?;
            }
        }
        _ => {}
    }
    Ok(())
}

pub(super) fn write_optional(path: Option<&Path>, value: &impl Serialize) -> Result<()> {
    if let Some(path) = path {
        storage::write(path, value)?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "files_tests.rs"]
mod tests;
