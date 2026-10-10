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
        return json!("[包含认证字段，内容已隐藏]");
    }
    let mut out: String = text.chars().take(limit).collect();
    if text.chars().count() > limit {
        out.push_str("\n…[摘要已截断]");
    }
    json!(out)
}

fn event(value: &Value, offset: u64) -> Option<Value> {
    let kind = value["type"].as_str()?;
    let item = &value["item"];
    let item_type = item["type"].as_str().unwrap_or("");
    let title = match (kind, item_type) {
        ("thread.started", _) => "Session 已建立",
        ("turn.started", _) => "开始处理任务",
        ("turn.completed", _) => "本轮执行结束",
        ("turn.failed" | "error", _) => "执行报告错误",
        ("item.started", "command_execution" | "mcp_tool_call" | "web_search") => "正在调用工具",
        ("item.completed", "command_execution" | "mcp_tool_call" | "web_search") => "工具执行结束",
        ("item.completed", "agent_message") => "Agent 公开消息",
        ("item.completed", "file_change") => "文件变更记录",
        // JSON bridge adapters may emit this optional, non-authoritative telemetry.
        ("session.activity", _) => "Agent 活动",
        _ => return None,
    };
    Some(
        json!({"id":offset,"type":kind,"title":title,"item_type":item_type,
        "item_id":item["id"].as_str().map(|s|s.chars().take(100).collect::<String>()),
        "session_id":value["thread_id"].as_str().map(|s|s.chars().take(100).collect::<String>()),
        "status":text_field(&item["status"],80),"exit_code":item["exit_code"].as_i64(),
        "text":if item_type=="agent_message" {text_field(&item["text"],4000)} else if value["message"].is_string() {text_field(&value["message"],1000)} else {text_field(&value["error"]["message"],1000)},
        "command":text_field(&item["command"],1200),
        "output":text_field(&item["aggregated_output"],2000),
        "trust":"untrusted_worker_telemetry","event_time":null}),
    )
}

pub(super) fn read(state: &Path, run: &str) -> Result<Value> {
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
    let start = size.saturating_sub(TAIL_BYTES);
    file.seek(SeekFrom::Start(start))?;
    let mut bytes = Vec::new();
    file.take(TAIL_BYTES).read_to_end(&mut bytes)?;
    let mut offset = start;
    let mut events = Vec::new();
    let mut invalid = 0;
    for (i, line) in bytes.split_inclusive(|b| *b == b'\n').enumerate() {
        let at = offset;
        offset += line.len() as u64;
        if (start > 0 && i == 0) || !line.ends_with(b"\n") {
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
    }
    Ok(json!({"run_id":run,"session_id":session_id,"bytes":size,
        "modified_at":metadata.modified()?.duration_since(UNIX_EPOCH)?.as_secs(),
        "events":events,"truncated":truncated,"invalid_lines":invalid,
        "notice":"stdio-json 公开事件摘要；时间为日志更新时间，原始事件未提供时间戳。"}))
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
    let mut current =
        read_small("controller-status.json").unwrap_or_else(|_| json!({"phase":"unknown"}));
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

pub(super) fn collect(c: &Config, shared: &Shared, stop: &AtomicBool) {
    let mut previous = Value::Null;
    while !stop.load(Ordering::Relaxed) {
        let jobs = shared.read().unwrap()["jobs"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let mut chosen = jobs.iter().collect::<Vec<_>>();
        chosen.sort_by_key(|j| j["state"] != "running");
        let sessions: Vec<_> = chosen
            .into_iter()
            .take(16)
            .filter_map(|j| read(&c.state_dir, j["run_id"].as_str()?).ok())
            .collect();
        let next = json!({"sessions":sessions,"controller":controller(&c.state_dir)});
        if next != previous {
            let mut data = shared.write().unwrap();
            data["sessions"] = next["sessions"].clone();
            data["controller"] = next["controller"].clone();
            data["revision"] = data["revision"]
                .as_u64()
                .unwrap_or(0)
                .saturating_add(1)
                .into();
            previous = next;
        }
        for _ in 0..10 {
            if stop.load(Ordering::Relaxed) {
                return;
            }
            thread::sleep(Duration::from_millis(100));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn session_tail_is_bounded_tolerates_partial_lines_and_rejects_traversal() {
        let root = std::env::temp_dir().join(format!(
            "lab-observer-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = Cleanup(root.clone());
        let path = root.join("runs/test-attempt-1/process/stdout.jsonl");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut log = File::create(&path).unwrap();
        writeln!(
            log,
            "{}",
            json!({"type":"thread.started","thread_id":{"secret":"DO_NOT_PROJECT"}})
        )
        .unwrap();
        writeln!(log, "{}", "x".repeat(TAIL_BYTES as usize)).unwrap();
        for index in 0..60 {
            writeln!(log, "{}", json!({"type":"item.completed","item":{"type":"agent_message","text":format!("public-{index}")}})).unwrap();
        }
        writeln!(log, "invalid JSON").unwrap();
        write!(log, "{{\"type\":\"item.completed\"").unwrap();
        drop(log);
        let observed = read(&root, "test-attempt-1").unwrap();
        assert!(observed["session_id"].is_null());
        assert_eq!(observed["truncated"], true);
        assert_eq!(observed["invalid_lines"], 1);
        let events = observed["events"].as_array().unwrap();
        assert_eq!(events.len(), MAX_EVENTS);
        assert_eq!(events[0]["text"], "public-12");
        assert_eq!(events.last().unwrap()["text"], "public-59");
        assert!(
            events
                .windows(2)
                .all(|pair| pair[0]["id"].as_u64() < pair[1]["id"].as_u64())
        );
        for run in [
            "",
            "../test-attempt-1",
            "test/attempt",
            "%2e%2e",
            "test.attempt",
        ] {
            assert!(read(&root, run).is_err());
        }
    }
    #[test]
    fn stdio_projection_keeps_public_activity_but_excludes_reasoning_and_credentials() {
        assert!(event(&json!({"type":"item.completed","item":{"type":"reasoning","text":"PRIVATE_REASONING"}}),0).is_none());
        let projected=event(&json!({"type":"item.completed","item":{"type":"command_execution","command":"curl -H 'Authorization: Bearer SECRET'","aggregated_output":"done","status":"completed","exit_code":0},"lease":"PRIVATE_LEASE"}),45).unwrap();
        assert_eq!(projected["id"], 45);
        assert_eq!(projected["exit_code"], 0);
        assert_eq!(projected["output"], "done");
        assert!(!projected.to_string().contains("SECRET"));
        assert!(!projected.to_string().contains("PRIVATE_LEASE"));
        assert_eq!(
            event(
                &json!({"type":"item.completed","item":{"type":"agent_message","text":"公开进度"}}),
                0
            )
            .unwrap()["text"],
            "公开进度"
        );
    }
}
