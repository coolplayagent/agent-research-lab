//! CLI integration only: relay-memory owns storage, retrieval and indexing.
use super::*;

fn home(c: &Config, id: &str) -> Result<PathBuf> {
    safe_id(id)?;
    let home = c.state_dir.join("people/memory").join(id);
    fs::create_dir_all(&home)?;
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&home, fs::Permissions::from_mode(0o700))?;
    Ok(home)
}
fn call(c: &Config, id: &str, operation: &str, arguments: &[String]) -> Result<Value> {
    let binding = c
        .tools
        .get("relay-memory")
        .context("relay-memory binding absent")?;
    ensure!(
        binding.binary.is_absolute(),
        "relay-memory must use an absolute configured executable"
    );
    let home = home(c, id)?;
    // Host serializes access. No agent receives this directory or another person's memory.
    let _lock = storage::lock(&home.join("host.lock"))?;
    let mut args = vec![
        "RELAY_MEMORY_BACKEND=sqlite".into(),
        binding
            .binary
            .to_str()
            .context("invalid memory binary path")?
            .into(),
        operation.into(),
        "--home".into(),
        home.display().to_string(),
    ];
    args.extend_from_slice(arguments);
    let output = process::capture("/usr/bin/env", &args, &c.workspace, Duration::from_secs(10))?;
    ensure!(
        output.status.success(),
        "relay-memory {operation} failed; inspect the configured runtime"
    );
    ensure!(
        output.stdout.len() <= 256 * 1024,
        "relay-memory response exceeds 256 KiB"
    );
    serde_json::from_slice(&output.stdout).context("relay-memory returned invalid JSON")
}
fn prepare(c: &Config, id: &str, session: &str, query: &str) -> Result<Value> {
    call(
        c,
        id,
        "prepare",
        &[
            "--session".into(),
            session.into(),
            "--prompt".into(),
            query.chars().take(3000).collect(),
            "--limit".into(),
            "3".into(),
        ],
    )
}
pub(super) fn capture(c: &Config, person: &Person, task: &Task) -> Result<PersonaMemory> {
    let pack = prepare(c, &person.id, &task.id, &task.prompt)?;
    let context: String = pack["bootstrap"]
        .as_str()
        .context("relay-memory pack lacks bootstrap")?
        .chars()
        .take(8000)
        .collect();
    ensure!(context.len() <= 32 * 1024, "memory context exceeds bound");
    let binary = &c.tools["relay-memory"].binary;
    Ok(PersonaMemory {
        sha256: storage::digest(context.as_bytes()),
        context,
        pack_sha256: storage::digest(&serde_json::to_vec(&pack)?),
        executable_sha256: storage::digest(&fs::read(binary)?),
        captured_at: now(),
    })
}
/// Explicitly requested recall, never part of the one-second observation loop.
pub fn inspect_memory(c: &Config, id: &str, query: &str) -> Result<Value> {
    let directory = directory(c)?;
    let person = directory.people.get(id).context("unknown person")?;
    let stats = call(c, id, "stats", &[])?;
    let pack = prepare(
        c,
        id,
        "profile-observer",
        if query.trim().is_empty() {
            &person.purpose
        } else {
            query
        },
    )?;
    let mut events: Vec<_> = pack["recent_events"].as_array().into_iter().flatten()
        .chain(pack["timeline_events"].as_array().into_iter().flatten())
        .take(12).map(|event| json!({"id":event["id"],"session":event["session_id"],"at":event["timestamp"],"summary":event["summary"],"response":event["response"],"metadata":event["metadata"]})).collect();
    let mut seen: BTreeSet<String> = events
        .iter()
        .filter_map(|e| e["id"].as_str().map(str::to_owned))
        .collect();
    for trace in pack["evidence_traces"].as_array().into_iter().flatten() {
        let Some(event_id) = trace["event_id"].as_str() else {
            continue;
        };
        if events.len() >= 12 {
            break;
        }
        if seen.insert(event_id.to_owned()) {
            events.push(json!({"id":event_id,"session":trace["session_id"],"at":trace["timestamp"],"summary":trace["summary"],"response":trace["raw_response_excerpt"],"excerpt":true,"metadata":{}}));
        }
    }
    let journal = c.state_dir.join("people/writebacks").join(id);
    let mut unknown = 0;
    let mut checked = 0;
    if journal.exists() {
        for entry in fs::read_dir(journal)?.take(8192) {
            let entry = entry?;
            if entry.file_type()?.is_file() && entry.path().extension().is_some_and(|e| e == "json")
            {
                let record: Value = storage::read(&entry.path())?;
                checked += 1;
                if record["state"] != "done" {
                    unknown += 1;
                }
            }
        }
    }
    Ok(
        json!({"person_id":id,"provider":"relay-memory","stats":stats,"events":events,"checked_writebacks":checked,"unconfirmed_writebacks":unknown,
        "notice":"记忆是历史线索。每次实验使用已固定的上下文；研究结论仍以 SuperPOD 和证据为准。"}),
    )
}
/// Host receipt journal prevents replay after success or an ambiguous process exit.
/// Unknown outcomes remain visible; only upstream can safely provide idempotent retries.
fn remember(
    c: &Config,
    id: &str,
    key: &str,
    session: &str,
    prompt: &str,
    response: &str,
    metadata: Value,
) -> Result<Value> {
    safe_id(id)?;
    let digest = storage::digest(&serde_json::to_vec(&json!([
        session, prompt, response, metadata
    ]))?);
    let path = c
        .state_dir
        .join("people/writebacks")
        .join(id)
        .join(format!("{}.json", storage::digest(key.as_bytes())));
    let _lock = storage::lock(&path.with_extension("lock"))?;
    if path.try_exists()? {
        let prior: Value = storage::read(&path)?;
        ensure!(
            prior["input_sha256"] == digest,
            "memory request key already binds different input"
        );
        ensure!(
            prior["state"] == "done",
            "memory write outcome unknown; reconciliation required"
        );
        return Ok(prior);
    }
    let mut record = json!({"state":"running","input_sha256":digest,"session":session,"at":now()});
    storage::write(&path, &record)?;
    let result = call(
        c,
        id,
        "remember",
        &[
            "--session".into(),
            session.into(),
            "--prompt".into(),
            prompt.into(),
            "--response".into(),
            response.into(),
            "--metadata".into(),
            serde_json::to_string(&metadata)?,
        ],
    );
    match result {
        Ok(event) => {
            record["state"] = json!("done");
            record["event_id"] = event["id"].clone();
            storage::write(&path, &record)?;
            Ok(record)
        }
        Err(error) => {
            record["state"] = json!("unknown");
            record["error"] = json!(error.to_string());
            storage::write(&path, &record)?;
            Err(error)
        }
    }
}
pub fn remember_note(c: &Config, id: &str, request_id: &str, note: &str) -> Result<Value> {
    ensure!(directory(c)?.people.contains_key(id), "unknown person");
    safe_id(request_id)?;
    ensure!(
        !note.trim().is_empty() && validate_text(note, 4096),
        "memory note must be 1..4096 bytes"
    );
    remember(
        c,
        id,
        &format!("note:{request_id}"),
        "profile-notes",
        "用户为数字人记录的偏好、研究线索与待解问题",
        note,
        json!({"origin":"user_note","request_id":request_id}),
    )
}
pub(in crate::people) fn writeback(c: &Config, job: &Job, receipt: &Value) -> Result<Value> {
    let Some(person) = &job.persona else {
        return Ok(json!({"state":"not_enabled"}));
    };
    // Persist a concise public result and immutable evidence references, not prompts,
    // private conversations, raw runs, holdouts, credentials or another person's store.
    let checkpoint: Value = serde_json::from_str(&memory_checkpoint(job, receipt)?)?;
    let mut narrative = checkpoint["summary"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    for (field, label) in [("findings", "发现"), ("limitations", "局限 / 待解问题")] {
        for line in checkpoint[field]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            narrative.push_str(&format!("\n{label}：{line}"));
        }
    }
    remember(
        c,
        &person.id,
        &format!("run:{}", job.run_id),
        &job.task.id,
        &remember_prompt(&job.task.prompt),
        &narrative,
        json!({"origin":"committed_research_receipt","person_id":person.id,"profile_revision":person.revision,"run_id":job.run_id,
            "source_commit":job.source_commit,"superpod_commit":job.superpod_commit,"prompt_digest":job.prompt_digest,"config_digest":job.config_digest,
            "receipt_sha256":storage::digest(&serde_json::to_vec(receipt)?),"memory_context_sha256":person.memory.as_ref().map(|m| &m.sha256)}),
    )
}
