//! Failure output remains in bounded private local storage, never in API errors.
use anyhow::Result;
use std::{fs, io::Read, path::Path};

static CAPTURE: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub(super) fn capture(
    state: &Path,
    output: &Path,
    identity: &str,
    exit_code: u32,
) -> Result<String> {
    let _capture = CAPTURE
        .lock()
        .map_err(|_| anyhow::anyhow!("sandbox diagnostic lock poisoned"))?;
    let mut previous = fs::read_dir(state)?
        .filter_map(Result::ok)
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.starts_with("sandbox-") && name.ends_with(".diagnostic.json")
        })
        .collect::<Vec<_>>();
    previous.sort_by_key(|entry| {
        entry
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(std::time::UNIX_EPOCH)
    });
    for entry in previous.iter().take(previous.len().saturating_sub(31)) {
        fs::remove_file(entry.path())?;
    }
    let id = format!("sandbox-{}", &storage::digest(identity.as_bytes())[..32]);
    let mut bytes = Vec::new();
    fs::File::open(output)?
        .take(64 * 1024)
        .read_to_end(&mut bytes)?;
    storage::write(
        &state.join(format!("{id}.diagnostic.json")),
        &serde_json::json!({"diagnostic_id":id,"exit_code":exit_code,"stderr":String::from_utf8_lossy(&bytes)}),
    )?;
    Ok(id)
}

#[cfg(test)]
#[path = "diagnostics_tests.rs"]
mod tests;
