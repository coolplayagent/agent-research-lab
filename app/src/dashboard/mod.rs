//! A bounded local collaboration observer with explicit identity management.
use anyhow::{Context, Result, ensure};
use config::Config;
use multi_agent::TeamMembership;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::Path,
    sync::{
        Arc, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use task::Job;
mod manage;
mod personas;
mod profiles;
mod sessions;

const MAX_JOBS: usize = 512;
const MAX_ENTRIES: usize = 8192;
const MAX_FILE: u64 = 512 * 1024;
const MAX_BOARDS: usize = 32;
const REFRESH: Duration = Duration::from_secs(5);

fn read_job(path: &Path) -> Result<Job> {
    let file = File::open(path)?;
    ensure!(file.metadata()?.is_file(), "job is not a regular file");
    let mut bytes = Vec::new();
    file.take(MAX_FILE + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= MAX_FILE,
        "job exceeds observer size limit"
    );
    let job: Job = serde_json::from_slice(&bytes)?;
    config::safe_id(&job.task.id)?;
    Ok(job)
}

fn selected_jobs(state: &Path) -> Result<(Vec<Job>, Vec<String>)> {
    let directory = state.join("jobs");
    if !directory.exists() {
        return Ok((vec![], vec![]));
    }
    let mut warnings = vec![];
    let mut paths = vec![];
    for (index, entry) in fs::read_dir(directory)?.enumerate() {
        if index == MAX_ENTRIES {
            warnings.push("目录超过 8192 项；当前只展示扫描范围内的任务。".into());
            break;
        }
        let entry = entry?;
        if entry.path().extension().is_some_and(|s| s == "json") && entry.file_type()?.is_file() {
            paths.push((entry.metadata()?.modified()?, entry.path()));
        }
    }
    paths.sort_by(|a, b| b.cmp(a));
    if paths.len() > MAX_JOBS {
        warnings.push(format!(
            "仅展示最近更新的 {MAX_JOBS} 个任务；统计与依赖图不包含更早记录。"
        ));
    }
    let mut jobs = vec![];
    for (_, path) in paths.into_iter().take(MAX_JOBS) {
        match read_job(&path) {
            Ok(job) => jobs.push(job),
            Err(_) => warnings.push(format!(
                "任务记录 {} 无法读取或超出大小限制。",
                path.file_name().unwrap_or_default().to_string_lossy()
            )),
        }
    }
    Ok((jobs, warnings))
}

fn clipped(value: &Value) -> Value {
    value
        .as_str()
        .map(|s| Value::String(s.chars().take(1200).collect()))
        .unwrap_or(Value::Null)
}

/// Explicit projection: never serialize a Job, backend credentials, lease, prompt,
/// raw workflow output, worker log or task memory to the browser.
fn project(job: &Job, row: &Value) -> Value {
    let receipt: Option<Value> = row["status"]["frames"]["1"]["nodes"]["task"]["outputs"]["result"]
        .as_str()
        .and_then(|s| serde_json::from_str(s).ok());
    let committed_context = receipt.as_ref().and_then(|r| r.get("communication"));
    let active_context = job.launch.as_ref().and_then(|l| l.communication.as_ref());
    let context = active_context
        .and_then(|c| serde_json::to_value(c).ok())
        .or_else(|| committed_context.cloned());
    let cohort = job
        .task
        .communication
        .as_ref()
        .and_then(|team| team.member(job).ok())
        .map(|m| m.cohort_id);
    let repositories: BTreeMap<_, _> = job
        .research_inputs
        .as_ref()
        .map(|i| {
            i.repositories
                .iter()
                .map(|(name, repo)| (name.clone(), repo.upstream.commit.clone()))
                .collect()
        })
        .unwrap_or_default();
    json!({
        "id":job.task.id,"role":job.task.role,"repository":job.task.repository,
        "model":job.model,"run_id":job.run_id,"attempt":job.attempt,
        "backend":job.backend.as_ref().map(|b| b.id.as_str()).unwrap_or("codex"),
        "profile":{"handle":format!("agent-{}", &storage::digest(job.task.id.as_bytes())[..12]),
            "prompt_version":job.task.prompt_version,"use_memory":job.task.use_memory,
            "max_attempts":job.task.max_attempts.unwrap_or(3),"required_tools":job.task.required_tools,
            "capabilities":job.backend.as_ref().map(|b| b.spec.capabilities())},
        "dependencies":job.task.dependencies,"write":job.task.write,
        "state":row["state"],"reason":clipped(&row["reason"]),
        "observation_error":!row["error"].is_null(),
        "source_commit":job.source_commit,"superpod_commit":job.superpod_commit,
        "prompt_digest":job.prompt_digest,"config_digest":job.config_digest,
        "repositories":repositories,"cohort_id":cohort,
        "team":job.task.communication.as_ref().map(|t| &t.id),
        "topics":job.task.communication.as_ref().map(|t| &t.topics),
        "context":context.map(|c| json!({"message_ids":c["context"]["message_ids"],
            "as_of":c["context"]["as_of"],"digest":c["context"]["digest"]})),
        "persona":job.persona.as_ref().map(|p|json!({"id":p.id,"name":p.name,"revision":p.revision,"soul":p.soul,"memory":p.memory.as_ref().map(|m|json!({"sha256":m.sha256,"pack_sha256":m.pack_sha256,"executable_sha256":m.executable_sha256,"captured_at":m.captured_at}))})),
        "postprocessing":row["postprocessing"],
        "candidate_commit":receipt.as_ref().map(|r| &r["candidate"]["candidate_commit"]),
        "communication_gap":job.launch.as_ref().and_then(|l| l.communication_gap.as_ref()),
        "report":receipt.as_ref().map(|r| json!({"summary":clipped(&r["agent_report"]["summary"]),
            "findings":r["agent_report"]["findings"].as_array().map(|a|a.iter().take(12).map(clipped).collect::<Vec<_>>()),
            "limitations":r["agent_report"]["limitations"].as_array().map(|a|a.iter().take(8).map(clipped).collect::<Vec<_>>())})),
    })
}

fn snapshot(c: &Config) -> Result<Value> {
    let started = Instant::now();
    let _deadline = process::deadline_scope(Duration::from_secs(20));
    let (jobs, mut warnings) = selected_jobs(&c.state_dir)?;
    let status = runtime::status_for_jobs(c, &jobs)?;
    let rows: BTreeMap<_, _> = status["jobs"]
        .as_array()
        .context("status lacks jobs")?
        .iter()
        .filter_map(|v| v["id"].as_str().map(|id| (id, v)))
        .collect();
    let mut projected: Vec<_> = jobs
        .iter()
        .map(|j| {
            project(
                j,
                rows.get(j.task.id.as_str())
                    .copied()
                    .unwrap_or(&Value::Null),
            )
        })
        .collect();
    let roster = personas::assign(c, &jobs, &mut projected)?;
    if projected.iter().any(|j| j["observation_error"] == true) {
        warnings.push("部分 workflow 状态读取失败或超过采样时限；未知状态不表示任务完成。".into());
    }
    let mut seen = BTreeSet::new();
    let ids: Vec<_> = projected
        .iter()
        .filter_map(|j| j["cohort_id"].as_str())
        .filter(|id| seen.insert(*id))
        .collect();
    if ids.len() > MAX_BOARDS {
        warnings.push(format!("仅展示最近任务关联的 {MAX_BOARDS} 个协作组。"));
    }
    let now = chrono::Utc::now().timestamp().max(0) as u64;
    let mut boards = vec![];
    if !ids.is_empty() {
        match communication::Board::open(&c.state_dir) {
            Ok(board) => {
                for id in ids.into_iter().take(MAX_BOARDS) {
                    match board.inspect(id, now) {
                        Ok(view) => boards.push(view),
                        // Queued jobs have a derived cohort but no board until launch.
                        Err(_) => warnings.push(format!(
                            "协作组 {} 尚未创建、已清理或暂不可读取。",
                            &id[..12]
                        )),
                    }
                }
            }
            Err(_) => warnings.push("共享板尚未创建或不可读取。".into()),
        }
    }
    Ok(
        json!({"schema_version":1,"sampled_at":now,"sample_duration_ms":started.elapsed().as_millis(),
        "refresh_seconds":5,"paused":status["paused"],"summary":status["summary"],
        "max_agents":c.max_agents,"jobs":projected,"boards":boards,"warnings":warnings,
        "knowledge":"SuperPOD","read_only":false,"identity_management":true,"roster":roster,"error":null}),
    )
}

type Shared = Arc<RwLock<Value>>;

fn collect(c: &Config, shared: &Shared, stop: &AtomicBool) {
    while !stop.load(Ordering::Relaxed) {
        match snapshot(c) {
            Ok(mut value) => {
                let mut data = shared.write().unwrap();
                for key in ["sessions", "controller"] {
                    value[key] = data[key].clone();
                }
                value["revision"] = data["revision"]
                    .as_u64()
                    .unwrap_or(0)
                    .saturating_add(1)
                    .into();
                *data = value;
            }
            Err(_) => {
                // Keep the last good sample, with an explicit failure and its old timestamp.
                let mut data = shared.write().unwrap();
                data["error"] = json!("采样失败；保留上次数据。请检查本机任务与 workflow 状态。");
                data["revision"] = data["revision"]
                    .as_u64()
                    .unwrap_or(0)
                    .saturating_add(1)
                    .into();
            }
        }
        for _ in 0..REFRESH.as_millis() / 100 {
            if stop.load(Ordering::Relaxed) {
                return;
            }
            thread::sleep(Duration::from_millis(100));
        }
    }
}

fn allowed_request(request: &str, addr: SocketAddr) -> Result<&str> {
    ensure!(request.ends_with("\r\n\r\n"), "incomplete headers");
    let mut lines = request.split("\r\n");
    let first: Vec<_> = lines
        .next()
        .unwrap_or_default()
        .split_whitespace()
        .collect();
    ensure!(
        first.len() == 3 && first[0] == "GET" && first[2] == "HTTP/1.1",
        "GET required"
    );
    let mut host = None;
    for line in lines.filter(|s| !s.is_empty()) {
        let (name, value) = line.split_once(':').context("invalid header")?;
        let value = value.trim();
        if name.eq_ignore_ascii_case("host") {
            ensure!(host.replace(value).is_none(), "duplicate host");
        }
        if name.eq_ignore_ascii_case("origin") {
            ensure!(
                value == format!("http://{addr}")
                    || value == format!("http://localhost:{}", addr.port()),
                "foreign origin"
            );
        }
        if name.eq_ignore_ascii_case("sec-fetch-site") {
            ensure!(value == "same-origin" || value == "none", "foreign site");
        }
    }
    ensure!(
        host == Some(addr.to_string().as_str())
            || host == Some(format!("localhost:{}", addr.port()).as_str()),
        "foreign host"
    );
    Ok(first[1])
}

fn respond(stream: &mut TcpStream, status: &str, mime: &str, body: &[u8]) -> Result<()> {
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\nContent-Security-Policy: default-src 'self'; connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'\r\n\r\n",
        body.len()
    )?;
    stream.write_all(body)?;
    Ok(())
}

fn handle(
    mut stream: TcpStream,
    addr: SocketAddr,
    shared: &Shared,
    state: &Path,
    workspace: &Path,
    stop: &AtomicBool,
    config: Option<&Config>,
) -> Result<()> {
    stream.set_read_timeout(Some(Duration::from_millis(500)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    let started = Instant::now();
    let mut data = Vec::new();
    let mut buffer = [0; 1024];
    while data.len() < 8192 && started.elapsed() < Duration::from_secs(1) {
        let size = stream.read(&mut buffer)?;
        if size == 0 {
            break;
        }
        data.extend_from_slice(&buffer[..size]);
        if data.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }
    if let Some(end) = data.windows(4).position(|w| w == b"\r\n\r\n") {
        let headers = std::str::from_utf8(&data[..end + 4])?;
        if headers.starts_with("POST ") {
            return if let Some(c) = config {
                manage::handle(&mut stream, c, headers, &data[end + 4..], addr)
            } else {
                respond(
                    &mut stream,
                    "403 Forbidden",
                    "text/plain",
                    b"Management unavailable",
                )
            };
        }
    }
    let route = std::str::from_utf8(&data)
        .ok()
        .and_then(|r| allowed_request(r, addr).ok());
    match route {
        Some("/api/events") => events(&mut stream, shared, stop),
        Some("/") => respond(
            &mut stream,
            "200 OK",
            "text/html; charset=utf-8",
            include_bytes!("index.html"),
        ),
        Some("/app.js") => respond(
            &mut stream,
            "200 OK",
            "text/javascript; charset=utf-8",
            include_bytes!("app.js"),
        ),
        Some("/people.js") => respond(
            &mut stream,
            "200 OK",
            "text/javascript; charset=utf-8",
            include_bytes!("people.js"),
        ),
        Some("/style.css") => respond(
            &mut stream,
            "200 OK",
            "text/css; charset=utf-8",
            include_bytes!("style.css"),
        ),
        Some("/api/snapshot") => {
            let body = serde_json::to_vec(&*shared.read().unwrap())?;
            respond(
                &mut stream,
                "200 OK",
                "application/json; charset=utf-8",
                &body,
            )
        }
        Some(path) if path == "/api/people" || path.starts_with("/api/people?after=") => {
            let c = config.context("identity service unavailable")?;
            let after = path.strip_prefix("/api/people?after=").unwrap_or("");
            match personas::list(c, after) {
                Ok(value) => respond(
                    &mut stream,
                    "200 OK",
                    "application/json; charset=utf-8",
                    &serde_json::to_vec(&value)?,
                ),
                Err(_) => respond(
                    &mut stream,
                    "400 Bad Request",
                    "application/json",
                    br#"{"error":"Identity directory unavailable"}"#,
                ),
            }
        }
        Some(path) if path.starts_with("/api/people/") => {
            let c = config.context("identity service unavailable")?;
            let (id, after) = path[12..]
                .split_once("?after=")
                .unwrap_or((&path[12..], ""));
            match personas::history(c, id, after, shared) {
                Ok(value) => respond(
                    &mut stream,
                    "200 OK",
                    "application/json; charset=utf-8",
                    &serde_json::to_vec(&value)?,
                ),
                Err(_) => respond(
                    &mut stream,
                    "404 Not Found",
                    "application/json",
                    br#"{"error":"Identity history unavailable"}"#,
                ),
            }
        }
        Some(path) if path.starts_with("/api/profile/") => {
            let run = &path[13..];
            let job = shared.read().unwrap()["jobs"]
                .as_array()
                .and_then(|jobs| jobs.iter().find(|j| j["run_id"] == run))
                .cloned()
                .or_else(|| {
                    personas::historical_job(state, run)
                        .ok()
                        .map(|j| project(&j, &json!({"state":"historical"})))
                });
            if let Some(job) = job {
                respond(
                    &mut stream,
                    "200 OK",
                    "application/json; charset=utf-8",
                    &serde_json::to_vec(&profiles::read(state, workspace, &job))?,
                )
            } else {
                respond(
                    &mut stream,
                    "404 Not Found",
                    "text/plain",
                    b"Unknown profile",
                )
            }
        }
        Some(path) if path.starts_with("/api/session/") => {
            let (run, query) = path[13..].split_once('?').unwrap_or((&path[13..], ""));
            let before = if query.is_empty() {
                None
            } else if let Some(offset) = query
                .strip_prefix("before=")
                .filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
                .and_then(|s| s.parse::<u64>().ok())
            {
                Some(offset)
            } else {
                return respond(
                    &mut stream,
                    "400 Bad Request",
                    "text/plain",
                    b"Invalid cursor",
                );
            };
            let known = shared.read().unwrap()["jobs"]
                .as_array()
                .is_some_and(|jobs| jobs.iter().any(|j| j["run_id"] == run));
            if known || personas::historical_job(state, run).is_ok() {
                match sessions::page(state, run, before) {
                    Ok(value) => respond(
                        &mut stream,
                        "200 OK",
                        "application/json; charset=utf-8",
                        &serde_json::to_vec(&value)?,
                    ),
                    Err(_) => respond(
                        &mut stream,
                        "404 Not Found",
                        "application/json",
                        b"{\"error\":\"session log unavailable\"}",
                    ),
                }
            } else {
                respond(
                    &mut stream,
                    "404 Not Found",
                    "text/plain",
                    b"Unknown session",
                )
            }
        }
        Some(_) => respond(&mut stream, "404 Not Found", "text/plain", b"Not found"),
        None => respond(
            &mut stream,
            "403 Forbidden",
            "text/plain",
            b"Local GET requests only",
        ),
    }
}

fn events(stream: &mut TcpStream, shared: &Shared, stop: &AtomicBool) -> Result<()> {
    stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream; charset=utf-8\r\nCache-Control: no-store\r\nConnection: close\r\nX-Accel-Buffering: no\r\nX-Content-Type-Options: nosniff\r\n\r\nretry: 1000\n\n")?;
    let deadline = Instant::now() + Duration::from_secs(45);
    let mut previous = None;
    let mut last_write = Instant::now();
    while Instant::now() < deadline && !stop.load(Ordering::Relaxed) {
        let update = {
            let data = shared.read().unwrap();
            let revision = data["revision"].as_u64().unwrap_or(0);
            (previous != Some(revision)).then(|| (revision, data.clone()))
        };
        if let Some((revision, data)) = update {
            write!(
                stream,
                "id: {revision}\nevent: snapshot\ndata: {}\n\n",
                serde_json::to_string(&data)?
            )?;
            previous = Some(revision);
            last_write = Instant::now();
        } else if last_write.elapsed() >= Duration::from_secs(5) {
            stream.write_all(b": heartbeat\n\n")?;
            last_write = Instant::now();
        }
        thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}

pub(crate) fn serve(c: &Config, listen: SocketAddr, max_seconds: u64) -> Result<Value> {
    ensure!(
        listen.ip().is_loopback(),
        "dashboard must bind a loopback address"
    );
    ensure!(
        (1..=43200).contains(&max_seconds),
        "dashboard lifetime must be 1..43200 seconds"
    );
    let listener = TcpListener::bind(listen)?;
    let addr = listener.local_addr()?;
    listener.set_nonblocking(true)?;
    let shared = Arc::new(RwLock::new(
        json!({"revision":0,"sampled_at":null,"jobs":[],"boards":[],"sessions":[],"summary":{},"warnings":[],"error":"正在读取首个运行快照…"}),
    ));
    let stop = AtomicBool::new(false);
    let clients = std::sync::atomic::AtomicUsize::new(0);
    eprintln!("Research dashboard: http://{addr} (identity management enabled, {max_seconds}s)");
    let result = thread::scope(|scope| -> Result<()> {
        scope.spawn(|| collect(c, &shared, &stop));
        scope.spawn(|| sessions::collect(c, &shared, &stop));
        let deadline = Instant::now() + Duration::from_secs(max_seconds);
        let result = (|| -> Result<()> {
            while Instant::now() < deadline {
                match listener.accept() {
                    Ok((stream, peer)) if peer.ip().is_loopback() => {
                        if clients.load(Ordering::Relaxed) >= 16 {
                            let mut stream = stream;
                            let _ = stream.set_write_timeout(Some(Duration::from_millis(100)));
                            let _ = respond(
                                &mut stream,
                                "503 Service Unavailable",
                                "text/plain",
                                b"Observer connection limit reached",
                            );
                        } else {
                            clients.fetch_add(1, Ordering::Relaxed);
                            let clients = &clients;
                            let shared = &shared;
                            let stop = &stop;
                            let state = &c.state_dir;
                            scope.spawn(move || {
                                let _ = handle(
                                    stream,
                                    addr,
                                    shared,
                                    state,
                                    &c.workspace,
                                    stop,
                                    Some(c),
                                );
                                clients.fetch_sub(1, Ordering::Relaxed);
                            });
                        }
                    }
                    Ok(_) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(25))
                    }
                    Err(e) => return Err(e.into()),
                }
            }
            Ok(())
        })();
        stop.store(true, Ordering::Relaxed);
        result
    });
    result?;
    Ok(
        json!({"stopped":true,"listen":addr.to_string(),"at":SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs()}),
    )
}

#[cfg(test)]
mod tests;
