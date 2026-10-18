//! Bounded experiment-local memory transport and continuation checkpoints.
use crate::receipts::validate_report;
use anyhow::{Context, Result, ensure};
use config::{Config, safe_id};
use native_effects::fs;
use serde_json::{Value, json};
use std::{path::Path, time::Duration};
use task::Job;

/// Bound the escaped JSON representation without cutting a Unicode character.
fn memory_excerpt(text: &str, escaped_bytes: usize) -> Result<(String, bool)> {
    if serde_json::to_string(text)?.len() <= escaped_bytes + 2 {
        return Ok((text.to_owned(), false));
    }
    let mut excerpt = String::new();
    let mut used = 0;
    for character in text.chars() {
        let cost = serde_json::to_string(&character)?.len() - 2;
        if used + cost + "…".len() > escaped_bytes {
            break;
        }
        excerpt.push(character);
        used += cost;
    }
    excerpt.push('…');
    Ok((excerpt, true))
}

/// Relay Memory stores a compact continuation checkpoint. Full provenance stays
/// in the immutable workflow receipt and is addressed by its SHA-256 digest.
pub(super) fn memory_checkpoint(job: &Job, receipt: &Value) -> Result<String> {
    let report = &receipt["agent_report"];
    validate_report(report)?;
    let (summary, mut truncated) = memory_excerpt(report["summary"].as_str().unwrap(), 512)?;
    let mut select = |field: &str, count: usize, bytes: usize| -> Result<Vec<String>> {
        let values = report[field]
            .as_array()
            .context("checkpoint field is not an array")?;
        truncated |= values.len() > count;
        values
            .iter()
            .take(count)
            .map(|value| {
                let (text, shortened) = memory_excerpt(
                    value.as_str().context("checkpoint item is not text")?,
                    bytes,
                )?;
                truncated |= shortened;
                Ok(text)
            })
            .collect()
    };
    let findings = select("findings", 3, 256)?;
    let limitations = select("limitations", 2, 192)?;
    let checkpoint = serde_json::to_string(&json!({
        "schema_version":1,
        "run_id":job.run_id,
        "receipt_sha256":storage::digest(&serde_json::to_vec(receipt)?),
        "source_commit":job.source_commit,
        "superpod_commit":job.superpod_commit,
        "prompt_digest":job.prompt_digest,
        "summary":summary,
        "findings":findings,
        "limitations":limitations,
        "truncated":truncated,
        "omitted_fields":["research_inputs","tools","sources","next_tasks"]
    }))?;
    ensure!(checkpoint.len() <= 4096, "memory checkpoint exceeds 4 KiB");
    Ok(checkpoint)
}

pub(super) fn remember_prompt(prompt: &str) -> String {
    let mut characters = prompt.chars();
    let mut bounded: String = characters.by_ref().take(512).collect();
    if characters.next().is_some() {
        bounded.pop();
        bounded.push('…');
    }
    bounded
}

pub(super) fn memory_call(
    c: &Config,
    job: &Job,
    dir: &Path,
    operation: &str,
    response: Option<&str>,
) -> Result<Value> {
    let binary = &c
        .tools
        .get("relay-memory")
        .context("relay-memory binding absent")?
        .binary;
    ensure!(
        binary.is_absolute(),
        "relay-memory must use an absolute configured executable"
    );
    // Host memory belongs to one experiment across attempts; the child only sees
    // the bounded prepared context and its separate ephemeral tool memory.
    safe_id(&job.task.id)?;
    let home = c.state_dir.join("task-memory").join(&job.task.id);
    fs::create_dir_all(&home)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&home, fs::Permissions::from_mode(0o700))?;
    }
    let mut args = vec![
        operation.into(),
        "--home".into(),
        home.display().to_string(),
        "--session".into(),
        job.task.id.clone(),
        "--prompt".into(),
        if operation == "remember" {
            remember_prompt(&job.task.prompt)
        } else {
            job.task.prompt.chars().take(8000).collect()
        },
    ];
    if let Some(response) = response {
        args.extend(["--response".into(), response.chars().take(32000).collect()]);
    }
    // Explicit local backend prevents inherited Neo4j configuration from sharing experiments.
    let mut local_args = vec![
        "RELAY_MEMORY_BACKEND=sqlite".into(),
        binary.to_str().context("non-UTF8 memory binary")?.into(),
    ];
    local_args.extend(args);
    let output = process::capture(
        "/usr/bin/env",
        &local_args,
        &job.worktree,
        Duration::from_secs(20),
    )?;
    ensure!(
        output.status.success(),
        "memory {operation} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    ensure!(
        output.stdout.len() <= 256 * 1024,
        "memory response exceeds 256 KiB"
    );
    let value: Value =
        serde_json::from_slice(&output.stdout).context("memory command returned invalid JSON")?;
    storage::write(&dir.join(format!("memory-{operation}.json")), &value)?;
    Ok(value)
}

#[cfg(test)]
#[path = "memory_tests.rs"]
mod tests;
