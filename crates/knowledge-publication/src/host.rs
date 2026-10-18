//! Bounded Git and external knowledge CLI calls; reject failed adapter responses.
use anyhow::{Context, Result, ensure};
use delivery::capture_text;
use serde_json::Value;
use std::path::Path;

pub(super) fn git(cwd: &Path, args: &[&str]) -> Result<String> {
    capture_text(
        "git",
        &args.iter().map(|v| (*v).into()).collect::<Vec<_>>(),
        cwd,
    )
}

pub(super) fn relay(binary: &Path, cwd: &Path, args: &[&str]) -> Result<Value> {
    let result = capture_text(
        binary.to_str().context("non-UTF-8 relay-knowledge path")?,
        &args.iter().map(|v| (*v).into()).collect::<Vec<_>>(),
        cwd,
    )?;
    let value: Value =
        serde_json::from_str(&result).context("relay-knowledge did not return JSON")?;
    ensure!(
        value.get("error_kind").is_none(),
        "relay-knowledge returned an error"
    );
    if args.starts_with(&["map", "validate"]) {
        ensure!(
            value["results"]
                .as_array()
                .is_some_and(|results| !results.is_empty()
                    && results.iter().all(|result| result["valid"] == true)),
            "map validation is incomplete or failed"
        );
    }
    Ok(value)
}
