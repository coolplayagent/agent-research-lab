//! A bounded local collaboration observer with explicit identity management.
use anyhow::{Context, Result, ensure};
use config::Config;
use multi_agent::TeamMembership;
use serde_json::{Value, json};
use std::future::IntoFuture;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::Read,
    net::{SocketAddr, TcpListener},
    path::Path,
    sync::{
        Arc, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use task::Job;
pub(crate) mod benchmark;
mod manage;
mod observe;
mod operator;
mod personas;
mod profiles;
mod resources;
mod rooms;
mod sessions;
mod transport;

const MAX_JOBS: usize = 512;
const MAX_ENTRIES: usize = 8192;
const MAX_FILE: u64 = 512 * 1024;
const MAX_BOARDS: usize = 32;

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
        "model":job.model,"run_id":job.run_id,"session_id":job.task.id,"execution_id":job.run_id,"attempt":job.attempt,
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
        "persona":job.persona.as_ref().map(|p|json!({"id":p.id,"name":p.name,"revision":p.revision,"soul":p.soul,"execution":p.execution,"memory":p.memory.as_ref().map(|m|json!({"sha256":m.sha256,"pack_sha256":m.pack_sha256,"executable_sha256":m.executable_sha256,"captured_at":m.captured_at}))})),
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
        "refresh_seconds":null,"observation_mode":"filesystem_events","paused":status["paused"],"summary":status["summary"],
        "lineage_revision":fs::metadata(c.state_dir.join("evolution/lineage.json")).ok().and_then(|m|m.modified().ok()).and_then(|t|t.duration_since(UNIX_EPOCH).ok()).map(|d|d.as_nanos().to_string()),
        "max_agents":c.max_agents,"jobs":projected,"boards":boards,"warnings":warnings,
        "knowledge":"SuperPOD","read_only":false,"identity_management":true,"roster":roster,"error":null}),
    )
}

type Shared = Arc<RwLock<Value>>;

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
    let (updates, _) = tokio::sync::watch::channel(0);
    let (shutdown, _) = tokio::sync::watch::channel(false);
    let hub = crystal::Hub::open(&c.state_dir.join("crystal"))?;
    let mut web = transport::Web::new(
        hub,
        addr,
        Some(Arc::new(c.clone())),
        shared.clone(),
        updates.clone(),
        shutdown.clone(),
    );
    web.operator = Some(Arc::new(operator::Operator::open(&c.state_dir, addr)?));
    eprintln!("Management link is in masked controller state: dashboard/access.json");
    eprintln!("Research dashboard: http://{addr} (event-driven, {max_seconds}s)");
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()?;
    thread::scope(|scope| -> Result<()> {
        scope.spawn(|| {
            if observe::run(c, &shared, &stop, &updates).is_err() {
                let mut data = shared.write().unwrap();
                data["error"] = json!("文件事件服务不可用");
                drop(data);
                updates.send_modify(|v| *v = v.wrapping_add(1));
            }
        });
        let result = runtime.block_on(async {
            let listener = tokio::net::TcpListener::from_std(listener)?;
            let halt = async move {
                tokio::time::sleep(Duration::from_secs(max_seconds)).await;
                shutdown.send_replace(true);
            };
            tokio::time::timeout(
                Duration::from_secs(max_seconds + 5),
                axum::serve(listener, transport::router(web))
                    .with_graceful_shutdown(halt)
                    .into_future(),
            )
            .await??;
            Ok::<_, anyhow::Error>(())
        });
        stop.store(true, Ordering::Relaxed);
        result
    })?;
    Ok(
        json!({"stopped":true,"listen":addr.to_string(),"at":SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs()}),
    )
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod transport_tests;
