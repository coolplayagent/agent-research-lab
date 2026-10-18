//! Bounded projections of stdio JSON. Provider reasoning, prompts and credentials
//! are never selected; public messages and tool summaries remain untrusted text.
use super::*;
use std::io::{Seek, SeekFrom};

const TAIL_BYTES: u64 = 256 * 1024;
const MAX_EVENTS: usize = 48;

pub(super) fn text_field(value: &Value, limit: usize) -> Value {
    let Some(text) = value.as_str() else {
        return Value::Null;
    };
    let lowered = text.to_ascii_lowercase();
    if [
        "authorization:",
        "bearer ",
        "api_key=",
        "api-key=",
        "access_token",
        "refresh_token",
        "private key",
        "sk-proj-",
        "ghp_",
    ]
    .iter()
    .any(|key| lowered.contains(key))
    {
        return json!("[redacted: authentication fields]");
    }
    let mut out: String = text.chars().take(limit).collect();
    if text.chars().count() > limit {
        out.push_str("\n...[truncated]");
    }
    json!(out)
}

fn event(value: &Value, offset: u64) -> Option<Value> {
    let kind = value["type"].as_str()?;
    let item = &value["item"];
    let item_type = item["type"].as_str().unwrap_or("");
    let title = match (kind, item_type) {
        ("thread.started", _) => contracts::SessionEventKind::SessionStarted,
        ("turn.started", _) => contracts::SessionEventKind::TurnStarted,
        ("turn.completed", _) => contracts::SessionEventKind::TurnCompleted,
        ("turn.failed" | "error", _) => contracts::SessionEventKind::ExecutionError,
        ("item.started", "command_execution" | "mcp_tool_call" | "web_search") => {
            contracts::SessionEventKind::ToolStarted
        }
        ("item.completed", "command_execution" | "mcp_tool_call" | "web_search") => {
            contracts::SessionEventKind::ToolCompleted
        }
        ("item.completed", "agent_message") => contracts::SessionEventKind::AgentMessage,
        ("item.completed", "file_change") => contracts::SessionEventKind::FileChange,
        // JSON bridge adapters may emit this optional, non-authoritative telemetry.
        ("session.activity", _) => contracts::SessionEventKind::Activity,
        _ => return None,
    };
    Some(
        json!({"id":offset,"type":kind,"title":title,"item_type":item_type,
        "item_id":item["id"].as_str().map(|s|s.chars().take(100).collect::<String>()),
        "session_id":value["thread_id"].as_str().map(|s|s.chars().take(100).collect::<String>()),
        "status":contracts::SessionItemStatus::from_value(&item["status"]).unwrap_or(contracts::SessionItemStatus::Unknown),"exit_code":item["exit_code"].as_i64(),
        "text":if item_type=="agent_message" {text_field(&item["text"],4000)} else if value["message"].is_string() {text_field(&value["message"],1000)} else {text_field(&value["error"]["message"],1000)},
        "command":text_field(&item["command"],1200),
        "output":text_field(&item["aggregated_output"],2000),
        "trust":"untrusted_worker_telemetry","event_time":null}),
    )
}

pub(super) fn read(state: &Path, run: &str) -> Result<Value> {
    page(state, run, None)
}

/// Read one bounded page ending at an exclusive byte offset. Cursors preserve
/// complete line boundaries so walking backward neither duplicates nor drops events.
pub(super) fn page(state: &Path, run: &str, before: Option<u64>) -> Result<Value> {
    ensure!(
        !run.is_empty()
            && run.len() <= 128
            && run
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b)),
        "invalid session ID"
    );
    let path = state.join("runs").join(run).join("process/stdout.jsonl");
    let mut file = File::open(&path)?;
    let metadata = file.metadata()?;
    ensure!(metadata.is_file(), "session log is not regular");
    let size = metadata.len();
    let mut prefix = vec![];
    (&mut file).take(4096).read_to_end(&mut prefix)?;
    let session_id = prefix
        .split(|b| *b == b'\n')
        .filter_map(|line| serde_json::from_slice::<Value>(line).ok())
        .find(|v| v["type"] == "thread.started")
        .and_then(|v| {
            v["thread_id"]
                .as_str()
                .map(|s| json!(s.chars().take(100).collect::<String>()))
        })
        .unwrap_or(Value::Null);
    let end = before.unwrap_or(size).min(size);
    let start = end.saturating_sub(TAIL_BYTES);
    let mut boundary = *b"\n";
    if start > 0 {
        file.seek(SeekFrom::Start(start - 1))?;
        file.read_exact(&mut boundary)?;
    } else {
        file.seek(SeekFrom::Start(0))?;
    }
    let mut bytes = Vec::new();
    file.take(end - start).read_to_end(&mut bytes)?;
    let mut offset = start;
    let mut older_end = start;
    let mut events = Vec::new();
    let mut invalid = 0;
    for (i, line) in bytes.split_inclusive(|b| *b == b'\n').enumerate() {
        let at = offset;
        offset += line.len() as u64;
        if i == 0 && boundary[0] != b'\n' {
            // Re-read this crossing line in the preceding page. A line larger
            // than the entire byte budget is skipped to ensure forward progress.
            older_end = if offset < end { offset } else { start };
            continue;
        }
        if !line.ends_with(b"\n") {
            continue;
        }
        match serde_json::from_slice::<Value>(line) {
            Ok(v) => {
                if let Some(e) = event(&v, at) {
                    events.push(e);
                }
            }
            Err(_) => invalid += 1,
        }
    }
    let truncated = start > 0 || events.len() > MAX_EVENTS;
    if events.len() > MAX_EVENTS {
        events.drain(..events.len() - MAX_EVENTS);
        older_end = events[0]["id"].as_u64().unwrap();
    }
    Ok(json!({"run_id":run,"session_id":session_id,"bytes":size,
        "modified_at":metadata.modified()?.duration_since(UNIX_EPOCH)?.as_secs(),
        "events":events,"truncated":truncated,"invalid_lines":invalid,
        "page_end":end,"next_before":(older_end > 0).then_some(older_end),
        "notice":contracts::ApiNotice::PublicEventScope.notice(json!({}))}))
}

pub(super) fn controller(state: &Path) -> Value {
    let read_small = |name: &str| -> Result<Value> {
        let mut bytes = Vec::new();
        File::open(state.join(name))?
            .take(65537)
            .read_to_end(&mut bytes)?;
        ensure!(bytes.len() <= 65536, "oversize status");
        Ok(serde_json::from_slice(&bytes)?)
    };
    let mut current = read_small("controller-status.json")
        .unwrap_or_else(|_| json!({"phase":contracts::ControllerPhase::Unknown}));
    let pid = current["pid"].as_u64().unwrap_or(0);
    let live = fs::read_to_string(format!("/proc/{pid}/stat"))
        .ok()
        .and_then(|s| s.rsplit_once(") ").map(|(_, v)| v.to_owned()))
        .is_some_and(|s| {
            let parts: Vec<_> = s.split_whitespace().collect();
            parts.first() != Some(&"Z")
                && parts.get(19).copied() == current["process_start"].as_str()
        });
    current["live"] = live.into();
    let age = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .saturating_sub(current["at"].as_u64().unwrap_or(0));
    current["heartbeat_age_seconds"] = age.into();
    current["stale"] = (live && age > 120).into();
    current["seed"] = read_small("seed-status.json")
        .unwrap_or_else(|_| read_small("seed-error.json").unwrap_or(Value::Null));
    current
}

pub(super) fn update(c: &Config, shared: &Shared) -> bool {
    let jobs = shared.read().unwrap()["jobs"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let mut chosen = jobs.iter().collect::<Vec<_>>();
    chosen.sort_by_key(|j| {
        contracts::ExecutionState::from_value(&j["state"])
            != Some(contracts::ExecutionState::Running)
    });
    let sessions: Vec<_> = chosen
        .into_iter()
        .take(16)
        .filter_map(|j| read(&c.state_dir, j["run_id"].as_str()?).ok())
        .collect();
    let next = json!({"sessions":sessions,"controller":controller(&c.state_dir)});
    let changed = {
        let prior = shared.read().unwrap();
        next["sessions"] != prior["sessions"] || next["controller"] != prior["controller"]
    };
    if changed {
        let mut data = shared.write().unwrap();
        data["sessions"] = next["sessions"].clone();
        data["controller"] = next["controller"].clone();
        data["revision"] = data["revision"]
            .as_u64()
            .unwrap_or(0)
            .saturating_add(1)
            .into();
        return true;
    }

    false
}

#[cfg(test)]
#[path = "sessions_tests.rs"]
mod tests;
