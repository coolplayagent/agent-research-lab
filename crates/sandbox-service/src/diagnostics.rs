//! Bounded, redacted diagnostics travel privately to the owning binding journal.
use std::{fs, io::Read, path::Path};

pub(super) fn failure(
    output: &Path,
    home: &Path,
    env: &[(String, String)],
    exit_code: Option<i32>,
) -> anyhow::Error {
    let mut secrets = env
        .iter()
        .filter(|(_, value)| value.len() >= 4)
        .map(|(_, value)| value.clone())
        .collect::<Vec<_>>();
    for relative in [
        ".codex/auth.json",
        ".claude/.credentials.json",
        ".claude.json",
        ".pi/agent/auth.json",
        ".dsh/auth.json",
    ] {
        if let Ok(bytes) = fs::read(home.join(relative))
            && let Ok(value) = serde_json::from_slice(&bytes)
        {
            collect(&value, &mut secrets);
        }
    }
    let mut bytes = Vec::new();
    if let Ok(file) = fs::File::open(output) {
        let _ = file.take(64 * 1024).read_to_end(&mut bytes);
    }
    let mut stderr = String::from_utf8_lossy(&bytes).into_owned();
    secrets.sort_by_key(|s| std::cmp::Reverse(s.len()));
    for secret in secrets {
        stderr = stderr.replace(&secret, "[redacted]");
    }
    // URLs can contain credentials, query tokens or private endpoint names.
    let stderr = stderr
        .split_whitespace()
        .map(|word| {
            if word.contains("://") {
                "[endpoint]"
            } else {
                word
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    service_protocol::PrivateDiagnostic(format!(
        "exit_code={exit_code:?}; stderr={}",
        stderr.chars().take(8000).collect::<String>()
    ))
    .into()
}
fn collect(value: &serde_json::Value, secrets: &mut Vec<String>) {
    match value {
        serde_json::Value::String(s) if s.len() >= 4 => secrets.push(s.clone()),
        serde_json::Value::Object(values) => {
            for value in values.values() {
                collect(value, secrets);
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                collect(value, secrets);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
#[path = "diagnostics_tests.rs"]
mod tests;
