//! Preserve existing workspace storage choices before selecting new-install defaults.
use anyhow::{Result, ensure};
use service_api::{Configuration, LocalRuntimeFailure};
use std::path::Path;

pub(super) fn load(state: &Path, development: bool) -> Result<Configuration> {
    let path = state.join("private/services.json");
    let _lock = storage::lock(&state.join("private/services-init.lock"))?;
    if path.exists() {
        return storage::read(&path);
    }
    // A missing configuration must not reinterpret an existing managed data directory.
    let private = state.join("private");
    ensure!(
        !std::fs::read_dir(&private)?.any(|entry| entry.is_ok_and(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("storage-owner-")
                || e.file_name()
                    .to_string_lossy()
                    .starts_with("managed-storage")
        })),
        LocalRuntimeFailure(contracts::LocalRuntimeErrorCode::DataMissing)
    );
    let config = if development || state.join("crystal").exists() {
        Configuration::defaults(state)
    } else {
        Configuration::local(state)
    };
    config.validate()?;
    storage::write(&path, &config)?;
    Ok(config)
}

#[cfg(test)]
#[path = "initialization_tests.rs"]
mod tests;
