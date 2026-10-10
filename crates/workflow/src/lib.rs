//! Adapter to tested workflow-cli 0.1.1 and 0.2.0 protocols. The external run store is the durable authority.
//! Read-only workers and managed writes deliberately use separate protocols.
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

static FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug)]
pub struct Workflow {
    pub binary: PathBuf,
    pub database: PathBuf,
    pub work_dir: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskSpec {
    pub id: String,
    pub payload: Value,
    pub write: bool,
    pub timeout_ms: u64,
}

impl Workflow {
    pub fn new(
        binary: impl Into<PathBuf>,
        database: impl Into<PathBuf>,
        work_dir: impl Into<PathBuf>,
    ) -> Self {
        Self {
            binary: binary.into(),
            database: database.into(),
            work_dir: work_dir.into(),
        }
    }

    /// Initialize only an absent database; an existing foreign/corrupt store is never replaced.
    pub fn initialize(&self) -> Result<Value> {
        fs::create_dir_all(&self.work_dir)?;
        if self.database.exists() {
            return self.run(&["storage-plan".into(), self.db()]);
        }
        if let Some(parent) = self.database.parent() {
            fs::create_dir_all(parent)?;
        }
        self.run(&["init".into(), self.db()])
    }

    /// Return actual runtime metadata and reject unverified wire contracts.
    pub fn version(&self) -> Result<Value> {
        let version = self.call(&["version".into(), "--format".into(), "json".into()])?;
        ensure!(
            matches!(version["version"].as_str(), Some("0.1.1" | "0.2.0")),
            "workflow adapter requires tested CLI 0.1.1 or 0.2.0"
        );
        Ok(version)
    }

    pub fn check_bundle(&self, bundle: &Value) -> Result<Value> {
        let path = self.save("bundle", bundle)?;
        self.call(&["kernel".into(), "check".into(), path])
    }

    /// Caller retains run_id and started_at_unix_ms for exact retries after a lost reply.
    pub fn start(
        &self,
        bundle: &Value,
        run_id: &str,
        inputs: Value,
        started_at_unix_ms: u64,
    ) -> Result<Value> {
        self.check_bundle(bundle)?;
        let request = json!({"schema_version":1,"bundle":bundle,"run_id":run_id,
            "inputs":inputs,"started_at_unix_ms":started_at_unix_ms});
        let path = self.save("start", &request)?;
        self.run(&["start".into(), self.db(), path])
    }

    /// Stable request on disk lets recovery retry this exact start without changing logical time.
    pub fn start_task(
        &self,
        run_id: &str,
        write: bool,
        inputs: Value,
        timeout_ms: u64,
    ) -> Result<Value> {
        let bundle = task_bundle(
            &[TaskSpec {
                id: "task".into(),
                payload: inputs,
                write,
                timeout_ms,
            }],
            false,
        )?;
        let stable_path = self
            .work_dir
            .join(format!("start-{}.json", hex(run_id.as_bytes())));
        let request = if stable_path.exists() {
            let request: Value = serde_json::from_slice(&fs::read(&stable_path)?)?;
            ensure!(
                request["bundle"] == bundle && request["run_id"] == run_id,
                "run identity already binds different task inputs"
            );
            request
        } else {
            let value = json!({"schema_version":1,"bundle":bundle,"run_id":run_id,
                "inputs":{},"started_at_unix_ms":now_ms()?});
            self.persist_at(&stable_path, &value)?;
            value
        };
        self.check_bundle(&request["bundle"])?;
        self.run(&[
            "start".into(),
            self.db(),
            stable_path.to_string_lossy().into_owned(),
        ])
    }

    /// Restart an already frozen task without applying today's timeout default.
    /// Absence is distinct from corruption or a different task binding.
    pub fn replay_task_start(
        &self,
        run_id: &str,
        write: bool,
        inputs: Value,
    ) -> Result<Option<Value>> {
        let path = self
            .work_dir
            .join(format!("start-{}.json", hex(run_id.as_bytes())));
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
            Ok(_) => {}
        }
        let retained: Value = storage::read(&path)?;
        let timeout_ms = retained["bundle"]["capabilities"][0]["timeout_ms"]
            .as_u64()
            .context("retained start lacks its original task timeout")?;
        let started_at = retained["started_at_unix_ms"]
            .as_u64()
            .context("retained start lacks its original start time")?;
        let bundle = task_bundle(
            &[TaskSpec {
                id: "task".into(),
                payload: inputs,
                write,
                timeout_ms,
            }],
            false,
        )?;
        ensure!(
            retained
                == json!({"schema_version":1,"bundle":bundle,"run_id":run_id,
                "inputs":{},"started_at_unix_ms":started_at}),
            "retained start does not bind the exact frozen task"
        );
        self.check_bundle(&bundle)?;
        Ok(Some(self.run(&[
            "start".into(),
            self.db(),
            path.to_string_lossy().into_owned(),
        ])?))
    }

    pub fn status(&self, run_id: &str) -> Result<Value> {
        self.run(&["status".into(), self.db(), run_id.into()])
    }

    pub fn verify(&self, run_id: &str) -> Result<Value> {
        self.run(&["verify".into(), self.db(), run_id.into()])
    }

    pub fn history(&self, run_id: &str, after: u64, limit: u64) -> Result<Value> {
        self.run(&[
            "execution-history".into(),
            self.db(),
            run_id.into(),
            after.to_string(),
            limit.to_string(),
        ])
    }

    pub fn effects(&self, run_id: &str) -> Result<Value> {
        self.run(&[
            "effects".into(),
            self.db(),
            run_id.into(),
            "0".into(),
            "100".into(),
        ])
    }

    /// Administrative events use caller-retained identity and optimistic revision.
    pub fn pause(
        &self,
        run_id: &str,
        event_id: &str,
        revision: u64,
        at_ms: u64,
        reason: &str,
    ) -> Result<Value> {
        self.admission("pause", run_id, event_id, revision, at_ms, reason)
    }

    pub fn resume(
        &self,
        run_id: &str,
        event_id: &str,
        revision: u64,
        at_ms: u64,
        reason: &str,
    ) -> Result<Value> {
        self.admission("resume", run_id, event_id, revision, at_ms, reason)
    }

    fn admission(
        &self,
        action: &str,
        run_id: &str,
        event_id: &str,
        revision: u64,
        at_ms: u64,
        reason: &str,
    ) -> Result<Value> {
        self.run(&[
            action.into(),
            self.db(),
            run_id.into(),
            event_id.into(),
            revision.to_string(),
            at_ms.to_string(),
            reason.into(),
        ])
    }

    pub fn acquire(
        &self,
        run_id: &str,
        owner: &str,
        acquisition_id: &str,
        ttl_ms: u64,
    ) -> Result<Value> {
        let request = self.save(
            "lease-request",
            &json!({"run_id":run_id,"owner":owner,
            "acquisition_id":acquisition_id,"ttl_ms":ttl_ms}),
        )?;
        self.run(&["acquire".into(), self.db(), request])
    }

    pub fn renew(&self, lease: &Value, ttl_ms: u64) -> Result<Value> {
        self.run(&[
            "renew".into(),
            self.db(),
            self.save("lease", lease)?,
            ttl_ms.to_string(),
        ])
    }

    pub fn release(&self, lease: &Value) -> Result<Value> {
        self.run(&["release".into(), self.db(), self.save("lease", lease)?])
    }

    /// Returns {type:"task",attempt:{request,grant,attempt_id,...}} or an idle/terminal reason.
    pub fn claim(&self, lease: &Value) -> Result<Value> {
        self.run(&["claim".into(), self.db(), self.save("lease", lease)?])
    }

    /// Returns {type:"call",attempt:{intent,attempt_id,...}} before any write may execute.
    pub fn effect_claim(&self, lease: &Value) -> Result<Value> {
        self.run(&["effect-claim".into(), self.db(), self.save("lease", lease)?])
    }

    pub fn finish(&self, lease: &Value, attempt: &Value, outputs: Value) -> Result<Value> {
        self.settle(
            lease,
            attempt,
            json!({"status":contracts::WorkflowState::Succeeded,"outputs":outputs,"evidence":[]}),
        )
    }

    /// An observed nonzero read-only process exit is a permanent business failure, not a retryable protocol error.
    pub fn fail(&self, lease: &Value, attempt: &Value, message: &str) -> Result<Value> {
        self.settle(
            lease,
            attempt,
            json!({"status":contracts::WorkflowState::Failed,"code":"process_failed",
            "class":"permanent","message":message,"evidence":[]}),
        )
    }

    fn settle(&self, lease: &Value, attempt: &Value, outcome: Value) -> Result<Value> {
        ensure!(
            attempt.get("request").is_some(),
            "read-only settlement requires claimed worker request"
        );
        let digest = text_field(&attempt["grant"], "request_digest")?;
        let result = json!({"protocol_version":1,"request_digest":digest,
            "completed_at_unix_ms":now_ms()?,"outcome":outcome});
        self.finish_result(lease, attempt, &result)
    }

    /// Exact result retry or typed-artifact result supplied by a host adapter.
    pub fn finish_result(&self, lease: &Value, attempt: &Value, result: &Value) -> Result<Value> {
        self.run(&[
            "finish".into(),
            self.db(),
            self.save("lease", lease)?,
            text_field(attempt, "attempt_id")?.into(),
            self.save("result", result)?,
        ])
    }

    pub fn effect_observe(
        &self,
        lease: &Value,
        attempt: &Value,
        observation: &Value,
    ) -> Result<Value> {
        self.run(&[
            "effect-observe".into(),
            self.db(),
            self.save("lease", lease)?,
            text_field(attempt, "attempt_id")?.into(),
            self.save("observation", observation)?,
        ])
    }

    /// Receipt bytes must be produced by the actual adapter and describe what it observed.
    /// A successful process alone does not establish research quality or approve promotion.
    pub fn finish_effect(
        &self,
        lease: &Value,
        attempt: &Value,
        resource_id: &str,
        receipt_path: &Path,
        outputs: Value,
    ) -> Result<Value> {
        ensure!(
            attempt["kind"] == "write",
            "applied receipt helper requires a prepared write"
        );
        ensure!(
            !resource_id.is_empty(),
            "effect resource identity is required"
        );
        let receipt_bytes = fs::read(receipt_path).context("read actual provider/host receipt")?;
        ensure!(
            !receipt_bytes.is_empty(),
            "empty effect receipt is not evidence"
        );
        let intent = &attempt["intent"];
        ensure!(intent.is_object(), "effect claim lacks durable intent");
        let observation = json!({"status":contracts::EffectState::Applied,"receipt":{
            "operation_key":text_field(intent,"operation_key")?,
            "intent_digest":format!("sha256:{}",hex(&serde_json::to_vec(intent)?)),
            "target":intent["policy"]["target"],"resource_id":resource_id,
            "provider_receipt":format!("sha256:{}",hex(&receipt_bytes)),"outputs":outputs}});
        self.effect_observe(lease, attempt, &observation)
    }

    /// Timeout, interrupted process, malformed output, or nonzero write exit may hide partial effects.
    pub fn unknown_effect(&self, lease: &Value, attempt: &Value, reason: &str) -> Result<Value> {
        self.effect_observe(
            lease,
            attempt,
            &json!({"status":contracts::EffectState::Unknown,"reason":reason}),
        )
    }

    /// Manual reconciliation requires real provider audit supplied by caller; no inferred resolution.
    pub fn resolve_effect(
        &self,
        lease: &Value,
        operation_key: &str,
        resolution: &Value,
    ) -> Result<Value> {
        self.run(&[
            "effect-resolve".into(),
            self.db(),
            self.save("lease", lease)?,
            operation_key.into(),
            self.save("resolution", resolution)?,
        ])
    }

    fn db(&self) -> String {
        self.database.to_string_lossy().into_owned()
    }

    fn run(&self, args: &[String]) -> Result<Value> {
        let mut all = vec!["run".into()];
        all.extend_from_slice(args);
        let response = self.call(&all)?;
        response
            .get("result")
            .cloned()
            .context("workflow reply lacks result envelope")
    }

    fn call(&self, args: &[String]) -> Result<Value> {
        let output = process::capture(
            self.binary.to_str().context("non-UTF8 workflow binary")?,
            args,
            &self.work_dir,
            Duration::from_secs(30),
        )?;
        if !output.status.success() {
            bail!(
                "workflow {:?} failed ({}): {} {}",
                args.first(),
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let value: Value =
            serde_json::from_slice(&output.stdout).context("invalid workflow JSON")?;
        ensure!(
            value.get("ok") != Some(&Value::Bool(false)),
            "workflow rejected request: {value}"
        );
        Ok(value)
    }

    fn save(&self, kind: &str, value: &Value) -> Result<String> {
        let bytes = serde_json::to_vec(value)?;
        let path = self
            .work_dir
            .join("protocol")
            .join(format!("{kind}-{}.json", hex(&bytes)));
        self.persist_at(&path, value)?;
        Ok(path.to_string_lossy().into_owned())
    }

    fn persist_at(&self, path: &Path, value: &Value) -> Result<()> {
        let parent = path.parent().context("protocol path has no parent")?;
        fs::create_dir_all(parent)?;
        let bytes = serde_json::to_vec(value)?;
        if path.exists() {
            ensure!(
                fs::read(path)? == bytes,
                "immutable protocol file conflicts: {}",
                path.display()
            );
            return Ok(());
        }
        let temporary = parent.join(format!(
            ".pending-{}-{}",
            std::process::id(),
            FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let result = (|| {
            let mut file = options.open(&temporary)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            // Publish only complete bytes; hard-link refuses to overwrite a concurrent publisher.
            match fs::hard_link(&temporary, path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    ensure!(
                        fs::read(path)? == bytes,
                        "immutable protocol file conflicts: {}",
                        path.display()
                    );
                }
                Err(error) => return Err(error.into()),
            }
            fs::File::open(parent)?.sync_all()?;
            Ok(())
        })();
        let _ = fs::remove_file(&temporary);
        result
    }
}

/// A frozen graph supports either sequential work or a fork/join barrier.
/// Every write has one durable attempt and no claimed target-side idempotency.
pub fn task_bundle(tasks: &[TaskSpec], parallel: bool) -> Result<Value> {
    ensure!(
        !tasks.is_empty() && tasks.len() <= 32,
        "workflow requires 1..32 tasks"
    );
    let mut seen = std::collections::BTreeSet::new();
    for task in tasks {
        ensure!(
            !task.id.is_empty()
                && task
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
            "invalid task id"
        );
        ensure!(
            !["done", "fork", "join"].contains(&task.id.as_str()) && seen.insert(&task.id),
            "duplicate/reserved task id"
        );
        ensure!(
            (1..=86_400_000).contains(&task.timeout_ms),
            "task timeout must be 1..86400000 ms"
        );
    }
    let identity = json!({"id":"agent-research-lab","version":format!("v1-{}", &hex(&serde_json::to_vec(&(tasks,parallel))?)[..24])});
    let field = json!({"required":true,"value_type":{"type":"string"}});
    let mut nodes = vec![];
    let mut edges = vec![];
    let mut capabilities = vec![];
    let mut capability_keys = std::collections::BTreeSet::new();
    let mut effects = vec![];
    for (index, task) in tasks.iter().enumerate() {
        let reference = json!({"id":if task.write {"lab.host-write"} else {"lab.host-read"},
            "version":format!("v1-t{}",task.timeout_ms)});
        let descriptor = json!({"schema_version":1,"capability":reference,
            "effects":if task.write {json!({"type":"write","idempotency":{"mode":"none"},"irreversible":true})} else {json!({"type":"read_only"})},
            "inputs":{"task":field},"outputs":{"result":field},"timeout_ms":task.timeout_ms,
            "error_codes":{"process_failed":"permanent","invalid_task":"invalid_input"},
            "usage":"Host executes the exact frozen task; output records actual process observations. Writes use effect ledger; no automatic retry or promotion implied."});
        if capability_keys.insert(serde_json::to_string(&reference)?) {
            capabilities.push(descriptor);
        }
        nodes.push(json!({"id":task.id,"kind":{"type":"task","capability":reference},
            "inputs":{"task":field},"outputs":{"result":field},
            "bindings":{"task":{"source":"literal","value":serde_json::to_string(&task.payload)?}}}));
        let next = if parallel {
            "join"
        } else {
            tasks.get(index + 1).map_or("done", |next| next.id.as_str())
        };
        edges.push(json!({"id":format!("after-{}",task.id),"from":task.id,"to":next,"route":{"type":"next"}}));
        if parallel {
            edges.push(json!({"id":format!("before-{}",task.id),"from":"fork","to":task.id,"route":{"type":"next"}}));
        }
        if task.write {
            effects.push(json!({"workflow":identity,"node_id":task.id,"policy":{
            "identity":{"id":"lab-local-write","version":format!("v1-t{}",task.timeout_ms)},
            "target":{"id":"lab-isolated-worktree","version":"1"},
            "call_identity":{"id":"lab-executor","version":"1"},
            "retry":{"max_calls":1,"initial_backoff_ms":1,"max_backoff_ms":1,"total_write_ms":task.timeout_ms}}}));
        }
    }
    nodes.push(json!({"id":"done","kind":{"type":"terminal","outcome":"succeeded"}}));
    if parallel {
        nodes.push(json!({"id":"fork","kind":{"type":"fork"}}));
        nodes.push(json!({"id":"join","kind":{"type":"join","mode":"all","remaining":"await"}}));
        edges.push(json!({"id":"joined","from":"join","to":"done","route":{"type":"next"}}));
    }
    let workflow = json!({"schema_version":1,"id":identity["id"],"version":identity["version"],
        "entry":if parallel {"fork"} else {tasks[0].id.as_str()},"nodes":nodes,"edges":edges});
    Ok(
        json!({"schema_version":1,"root":identity,"workflows":[workflow],
        "capabilities":capabilities,"effect_bindings":effects}),
    )
}

/// Accept a status read or a mutation receipt, never infer success from CLI exit status.
pub fn status_name(value: &Value) -> Result<&str> {
    let snapshot = value.get("snapshot").unwrap_or(value);
    text_field(snapshot, "status")
}

pub fn now_ms() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_millis()
        .try_into()?)
}
fn hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn text_field<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .with_context(|| format!("workflow response missing {key}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bundle_identity_changes_with_prompts_and_effects() {
        let task = TaskSpec {
            id: "research".into(),
            payload: json!({"prompt":"A"}),
            write: false,
            timeout_ms: 60000,
        };
        let first = task_bundle(std::slice::from_ref(&task), false).unwrap();
        let mut changed = task.clone();
        changed.payload = json!({"prompt":"B"});
        assert_ne!(
            first["root"],
            task_bundle(&[changed], false).unwrap()["root"]
        );
        let mut write = task;
        write.write = true;
        let effect = task_bundle(&[write], false).unwrap();
        assert_eq!(
            effect["effect_bindings"][0]["policy"]["retry"]["max_calls"],
            1
        );
        assert_eq!(
            effect["capabilities"][0]["effects"]["idempotency"]["mode"],
            "none"
        );
    }
    #[test]
    fn immutable_protocol_inputs_are_not_overwritten() {
        let temp = tempfile::tempdir().unwrap();
        let workflow = Workflow::new("unused", temp.path().join("db"), temp.path());
        let path = temp.path().join("request.json");
        workflow.persist_at(&path, &json!({"request":1})).unwrap();
        assert!(workflow.persist_at(&path, &json!({"request":2})).is_err());
        assert_eq!(fs::read_to_string(path).unwrap(), "{\"request\":1}");
    }
    #[test]
    fn rejects_ambiguous_task_ids_and_empty_workflows() {
        assert!(task_bundle(&[], false).is_err());
        let task = TaskSpec {
            id: "done".into(),
            payload: json!({}),
            write: false,
            timeout_ms: 1,
        };
        assert!(task_bundle(&[task], false).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn fake_transport_keeps_business_failure_and_rejects_invalid_json() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let binary = temp.path().join("fake-workflow");
        fs::write(
            &binary,
            "#!/bin/sh\nprintf '%s\\n' '{\"ok\":true,\"result\":{\"status\":\"failed\"}}'\n",
        )
        .unwrap();
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
        let workflow = Workflow::new(&binary, temp.path().join("db"), temp.path());
        assert_eq!(
            status_name(&workflow.status("test").unwrap()).unwrap(),
            "failed"
        );
        fs::write(&binary, "#!/bin/sh\nprintf invalid-json\n").unwrap();
        assert!(workflow.status("test").is_err());
    }
    /// Explicit opt-in because this invokes an installed external CLI, never a simulated worker.
    #[test]
    #[ignore = "set LAB_WORKFLOW_BIN and run with --ignored for actual durable protocol integration"]
    fn actual_runtime_read_write_pause_and_uncertainty() {
        let binary = std::env::var("LAB_WORKFLOW_BIN").expect("LAB_WORKFLOW_BIN");
        let temp = tempfile::tempdir().unwrap();
        let w = Workflow::new(binary, temp.path().join("runs.db"), temp.path());
        w.version().unwrap();
        w.initialize().unwrap();
        w.start_task("read", false, json!({"command":"/bin/true"}), 60000)
            .unwrap();
        let retained_path = temp.path().join(format!("start-{}.json", hex(b"read")));
        let retained_bytes = fs::read(&retained_path).unwrap();
        assert!(
            w.start_task("read", false, json!({"command":"/bin/true"}), 90000)
                .is_err()
        );
        assert!(
            w.replay_task_start("read", false, json!({"command":"/bin/true"}))
                .unwrap()
                .is_some()
        );
        assert_eq!(fs::read(&retained_path).unwrap(), retained_bytes);
        assert!(
            w.replay_task_start("read", true, json!({"command":"/bin/true"}))
                .is_err()
        );
        assert!(
            w.replay_task_start("read", false, json!({"command":"changed"}))
                .is_err()
        );
        let state = w.status("read").unwrap();
        w.pause(
            "read",
            "pause-1",
            state["revision"].as_u64().unwrap(),
            now_ms().unwrap(),
            "test pause",
        )
        .unwrap();
        let state = w.status("read").unwrap();
        w.resume(
            "read",
            "resume-1",
            state["revision"].as_u64().unwrap(),
            now_ms().unwrap(),
            "test resume",
        )
        .unwrap();
        let lease = w.acquire("read", "test", "read-1", 300000).unwrap();
        let claim = w.claim(&lease).unwrap();
        let output =
            process::capture("/bin/true", &[], temp.path(), Duration::from_secs(2)).unwrap();
        assert!(output.status.success());
        let settled = w
            .finish(
                &lease,
                &claim["attempt"],
                json!({"result":"actual /bin/true exited 0"}),
            )
            .unwrap();
        assert_eq!(settled["snapshot"]["status"], "succeeded");
        w.release(&lease).unwrap();
        w.verify("read").unwrap();
        w.start_task("write", true, json!({"output":"actual.txt"}), 60000)
            .unwrap();
        let lease = w.acquire("write", "test", "write-1", 300000).unwrap();
        let claim = w.effect_claim(&lease).unwrap();
        let path = temp.path().join("actual.txt");
        let output = process::capture(
            "/bin/sh",
            &[
                "-c".into(),
                "printf actual-write > \"$1\"".into(),
                "test".into(),
                path.to_string_lossy().into_owned(),
            ],
            temp.path(),
            Duration::from_secs(2),
        )
        .unwrap();
        assert!(output.status.success());
        let settled = w
            .finish_effect(
                &lease,
                &claim["attempt"],
                path.to_str().unwrap(),
                &path,
                json!({"result":"actual-write"}),
            )
            .unwrap();
        assert_eq!(settled["snapshot"]["status"], "succeeded");
        w.release(&lease).unwrap();
        w.verify("write").unwrap();
        w.start_task("uncertain", true, json!({"output":"partial.txt"}), 60000)
            .unwrap();
        let lease = w
            .acquire("uncertain", "test", "uncertain-1", 300000)
            .unwrap();
        let claim = w.effect_claim(&lease).unwrap();
        let output = process::capture(
            "/bin/sh",
            &["-c".into(), "printf partial > partial.txt; exit 7".into()],
            temp.path(),
            Duration::from_secs(2),
        )
        .unwrap();
        assert!(!output.status.success());
        w.unknown_effect(
            &lease,
            &claim["attempt"],
            "actual process exited 7 after writing partial.txt",
        )
        .unwrap();
        let next = w.effect_claim(&lease).unwrap();
        assert_ne!(next["type"], "call");
        assert_ne!(w.status("uncertain").unwrap()["status"], "succeeded");
        w.release(&lease).unwrap();
        w.verify("uncertain").unwrap();
        let tasks = [
            TaskSpec {
                id: "first".into(),
                payload: json!({}),
                write: false,
                timeout_ms: 1000,
            },
            TaskSpec {
                id: "second".into(),
                payload: json!({}),
                write: true,
                timeout_ms: 1000,
            },
        ];
        w.check_bundle(&task_bundle(&tasks, true).unwrap()).unwrap();
    }
}
