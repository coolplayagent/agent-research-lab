//! Guarded production scheduling with a test-owned external owner, no model.
use super::*;
use std::os::unix::fs::PermissionsExt;

thread_local! {
    static DIRECT_AGENT_FIXTURE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub(crate) fn direct_agent_fixture() -> bool {
    DIRECT_AGENT_FIXTURE.get()
}

pub(crate) struct DirectAgentFixture;

impl DirectAgentFixture {
    pub(crate) fn enter() -> Self {
        assert!(!DIRECT_AGENT_FIXTURE.replace(true));
        Self
    }
}

impl Drop for DirectAgentFixture {
    fn drop(&mut self) {
        DIRECT_AGENT_FIXTURE.set(false);
    }
}

#[derive(Default)]
pub(crate) struct Hook {
    pub(crate) stopped: bool,
    pub(crate) acquisitions: usize,
    pub(crate) model_entries: usize,
    pub(crate) allow_model_entry: bool,
    pub(crate) admission_delay: Duration,
}

impl admission::ScopedAdmissionHook for Hook {
    fn register_cleanup_domain(&mut self, _: &Config, _: &Path, _: u64) -> Result<()> {
        Ok(())
    }

    fn prepare(&mut self, _: &Config, _: &[Job]) -> Result<()> {
        Ok(())
    }

    fn take_acquire_preconditions(
        &mut self,
        _: &Config,
        _: &Job,
    ) -> Result<contracts::AcquirePreconditionsV1> {
        self.acquisitions += 1;
        let digest = format!("sha256:{}", "a".repeat(64));
        Ok(contracts::AcquirePreconditionsV1 {
            expected_run_digest: digest.clone(),
            expected_bundle_digest: digest.clone(),
            expected_snapshot_digest: digest.clone(),
            expected_execution_digest: digest,
            expected_snapshot_revision: 1,
        })
    }

    fn before_model_spawn(&mut self, _: &Config, _: &Job) -> Result<()> {
        self.model_entries += 1;
        std::thread::sleep(self.admission_delay);
        if self.allow_model_entry {
            return Ok(());
        }
        bail!("fixture must not reach model admission")
    }

    fn revoke(&mut self, _: &str) {
        self.stopped = true;
    }

    fn should_stop(&self) -> bool {
        self.stopped
    }
}

pub(crate) fn fixture(mode: &str) -> (tempfile::TempDir, Config, Job, PathBuf) {
    let (temp, mut config, mut job, _) = tests::completion_fixture(false, json!([]));
    let model = temp.path().join("model-fixture");
    fs::write(
        &model,
        format!(
            "#!/bin/sh\nprintf 'unexpected model' > '{}'\nexit 99\n",
            temp.path().join("model-started").display()
        ),
    )
    .unwrap();
    fs::set_permissions(&model, fs::Permissions::from_mode(0o700)).unwrap();
    config.codex = model.display().to_string();
    config.tools = [
        "workflow-cli",
        "relay-knowledge",
        "into-markdown",
        "qualitygate-cli",
        "computer-use-cli",
        "relay-memory",
        "repo-sandbox",
    ]
    .into_iter()
    .map(|name| {
        (
            name.into(),
            config::Tool {
                binary: PathBuf::from("/bin/true"),
                repository: config.superpod.clone(),
                probe: vec![],
            },
        )
    })
    .collect();
    fs::write(
        &config.workflow,
        r#"#!/usr/bin/python3
import json, pathlib, sys, time
root = pathlib.Path(__file__).parent
args = sys.argv[1:]
mode = (root / 'mode').read_text()
operation = args[1] if args[0] == 'run' else args[0]
with (root / 'events').open('a') as out:
    out.write(operation + '\n')
if operation == mode or (mode == 'claim-error' and operation == 'claim'):
    path = root / 'state/jobs/synthesis-recovery.json'
    job = json.loads(path.read_text())
    job['attempt'] = 2
    job['run_id'] = job['task']['id'] + '-attempt-2'
    job['last_error'] = 'independent current attempt'
    job['retry_after'] = None
    job['launch'] = None
    changed = json.dumps(job).encode()
    path.write_bytes(changed)
    (root / 'changed-job').write_bytes(changed)
if operation == 'version':
    print(json.dumps({'version': '0.2.0'}))
    sys.exit(0)
if operation == 'acquire-checked-v1':
    if mode == 'unknown':
        print('reply lost after possible commit')
        sys.exit(12)
    if mode == 'conflict':
        print(json.dumps({'ok': False, 'error': {'code': 'admission_conflict',
            'message': 'baseline differs', 'kernel_code': None}}))
        sys.exit(1)
    lease = json.loads(pathlib.Path(args[3]).read_text())['lease']
    ttl = lease.pop('ttl_ms')
    issued = int(time.time() * 1000)
    result = dict(lease, generation='fixture', epoch=1,
        issued_at_unix_ms=issued, expires_at_unix_ms=issued + ttl)
elif operation == 'claim':
    if mode in ('claim-error', 'release', 'release-error'):
        sys.exit(13)
    result = {'attempt': {'request': {}, 'grant': {'request_digest': 'fixture'},
        'attempt_id': 'fixture-attempt'}}
elif operation == 'release' and mode == 'release-error':
    sys.exit(14)
elif operation == 'finish' and mode == 'finish-error':
    sys.exit(15)
elif operation == 'status':
    result = {'status': 'running'}
else:
    result = {}
print(json.dumps({'ok': True, 'result': result}))
"#,
    )
    .unwrap();
    fs::write(temp.path().join("mode"), mode).unwrap();
    job.task.max_attempts = Some(3);
    job.prompt_digest = storage::digest(rendered_job_prompt(&config, &job).unwrap().as_bytes());
    job.config_digest = storage::digest(&serde_json::to_vec(&config).unwrap());
    storage::write(&job_path(&config, &job.task.id), &job).unwrap();
    storage::write(&config.state_dir.join("budget.json"), &Ledger::default()).unwrap();
    let scope = config.state_dir.join("private/guarded-scope.json");
    storage::write(&scope, &vec![&job.task.id]).unwrap();
    (temp, config, job, scope)
}

pub(crate) fn events(temp: &tempfile::TempDir) -> Vec<String> {
    fs::read_to_string(temp.path().join("events"))
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect()
}

#[test]
fn guarded_owner_acquire_and_claim_drift_preserve_the_new_complete_job() {
    for mode in ["acquire-checked-v1", "claim", "claim-error", "release"] {
        let (temp, config, job, scope) = fixture(mode);
        let mut hook = Hook::default();
        let error =
            run_scoped_impl(&config, false, 20, false, Some(&scope), Some(&mut hook)).unwrap_err();
        assert!(
            format!("{error:#}").contains("frozen job authority changed"),
            "{mode}: {error:#}"
        );
        assert_eq!(
            fs::read(job_path(&config, &job.task.id)).unwrap(),
            fs::read(temp.path().join("changed-job")).unwrap(),
            "{mode}: error settlement overwrote the independent Job"
        );
        let current: Job = storage::read(&job_path(&config, &job.task.id)).unwrap();
        assert_eq!(current.attempt, 2);
        assert!(current.launch.is_none());
        assert_eq!(hook.acquisitions, 1);
        assert!(hook.stopped);
        assert_eq!(hook.model_entries, 0);
        assert!(!temp.path().join("model-started").exists());
        let observed = events(&temp);
        assert_eq!(observed.iter().filter(|e| *e == "release").count(), 1);
        assert!(!observed.iter().any(|e| e == "settle"));
        assert_eq!(
            observed.iter().filter(|e| *e == "claim").count(),
            usize::from(mode != "acquire-checked-v1")
        );
    }
}

#[test]
fn guarded_unknown_and_conflict_never_schedule_a_new_attempt() {
    for mode in ["unknown", "conflict", "release-error"] {
        let (temp, config, job, scope) = fixture(mode);
        let mut hook = Hook::default();
        run_scoped_impl(&config, false, 20, false, Some(&scope), Some(&mut hook)).unwrap();
        let current: Job = storage::read(&job_path(&config, &job.task.id)).unwrap();
        assert_eq!(current.attempt, 1);
        assert_eq!(current.task.max_attempts, Some(3));
        assert!(current.retry_after.is_none(), "{mode}");
        let error = current.last_error.as_ref().unwrap();
        assert!(error.contains("reconciliation"), "{error}");
        if mode == "unknown" {
            assert!(error.contains("owner_acquire_unknown"), "{error}");
        }
        if mode == "release-error" {
            assert!(
                error.contains("lease release requires reconciliation"),
                "{error}"
            );
        }
        assert!(hook.stopped);
        let mut next_hook = Hook::default();
        run_scoped_impl(
            &config,
            false,
            20,
            false,
            Some(&scope),
            Some(&mut next_hook),
        )
        .unwrap();
        let after: Job = storage::read(&job_path(&config, &job.task.id)).unwrap();
        assert_eq!(after.attempt, 1);
        assert_eq!(next_hook.acquisitions, 0);
        assert_eq!(
            events(&temp)
                .iter()
                .filter(|e| *e == "acquire-checked-v1")
                .count(),
            1
        );
        assert_eq!(hook.model_entries, 0);
        assert!(!temp.path().join("model-started").exists());
    }
}

#[test]
fn guarded_writes_track_each_successful_full_snapshot_and_reject_drift() {
    let (_temp, config, mut job, _) = fixture("unknown");
    let mut writes = admission::JobWrites::new(&job, true);
    job.last_error = Some("first confirmed write".into());
    writes.persist(&config, &job).unwrap();
    job.last_error = Some("second confirmed write".into());
    writes.persist(&config, &job).unwrap();
    let mut advanced = job.clone();
    // Same attempt is insufficient: every frozen field must still match.
    advanced.model = "different-model".into();
    storage::write(&job_path(&config, &job.task.id), &advanced).unwrap();
    let original = fs::read(job_path(&config, &job.task.id)).unwrap();
    job.last_error = None;
    assert!(writes.persist(&config, &job).is_err());
    assert_eq!(fs::read(job_path(&config, &job.task.id)).unwrap(), original);
}

#[test]
fn guarded_error_settlement_rejoins_after_owner_finish_and_release() {
    for mode in ["finish", "release"] {
        let (temp, config, mut job, _) = fixture(mode);
        let dir = config.state_dir.join("runs").join(&job.run_id);
        fs::create_dir_all(&dir).unwrap();
        job.launch = Some(Launch {
            pid: 0,
            process_start: String::new(),
            lease: json!({"run_id":job.run_id}),
            attempt: json!({"request":{},"grant":{"request_digest":"fixture"},"attempt_id":"fixture-attempt"}),
            log_dir: dir,
            tools: json!({}),
            git_dir: String::new(),
            backend_request_sha256: None,
            communication: None,
            communication_gap: None,
            communication_member: None,
        });
        storage::write(&job_path(&config, &job.task.id), &job).unwrap();
        let mut writes = admission::JobWrites::new(&job, true);
        assert!(
            admission::launch_failed(
                &config,
                &workflow(&config),
                &mut job,
                &mut writes,
                &anyhow::anyhow!("test-owned launch failure"),
                true,
            )
            .is_err()
        );
        assert_eq!(
            fs::read(job_path(&config, &job.task.id)).unwrap(),
            fs::read(temp.path().join("changed-job")).unwrap()
        );
        assert_eq!(events(&temp), ["finish", "release"]);
        assert!(!temp.path().join("model-started").exists());
    }
}

#[test]
fn ordinary_launch_failure_preserves_existing_read_retry_behavior() {
    let (_temp, config, mut job, _) = fixture("unknown");
    let mut writes = admission::JobWrites::new(&job, false);
    admission::launch_failed(
        &config,
        &workflow(&config),
        &mut job,
        &mut writes,
        &anyhow::anyhow!("ordinary launch failure"),
        false,
    )
    .unwrap();
    assert!(job.retry_after.is_some());
    assert_eq!(job.attempt, 1);
    let current: Job = storage::read(&job_path(&config, &job.task.id)).unwrap();
    assert_eq!(current.retry_after, job.retry_after);
}

#[test]
fn late_successful_hook_cannot_extend_launch_or_task_deadline() {
    // Only namespace command construction is substituted. Acquisition, claim,
    // Job writes, final admission and the actual Process::spawn path are shared.
    let _direct_agent = DirectAgentFixture::enter();
    for (remaining, task_timeout, delay, may_spawn) in [
        (20, 20, Duration::ZERO, true),
        (3, 20, Duration::from_millis(3100), false),
        (20, 1, Duration::from_millis(1100), false),
    ] {
        let (temp, mut config, mut job, _) = fixture("late-hook");
        config.task_timeout_seconds = task_timeout;
        // Keep the harmless positive child alive long enough for identity join.
        // This is not a model, namespace or provider invocation.
        fs::write(
            &config.codex,
            format!(
                "#!/bin/sh\nprintf 'harmless fixture' > '{}'\nsleep 0.2\nexit 99\n",
                temp.path().join("model-started").display()
            ),
        )
        .unwrap();
        job.config_digest = storage::digest(&serde_json::to_vec(&config).unwrap());
        storage::write(&job_path(&config, &job.task.id), &job).unwrap();
        let mut hook = Hook {
            allow_model_entry: true,
            admission_delay: delay,
            ..Hook::default()
        };
        let mut writes = admission::JobWrites::new(&job, true);
        let owner = workflow(&config);
        let result = launch_guarded(
            &config,
            &owner,
            &mut job,
            remaining,
            &mut Some(&mut hook),
            &mut writes,
        );
        assert_eq!(hook.model_entries, 1, "fixture must reach the final hook");
        assert!(!hook.stopped, "late hook deliberately reports no stop");
        if may_spawn {
            let (mut process, mut heartbeat) = result.unwrap();
            let until = std::time::Instant::now() + Duration::from_secs(3);
            let status = loop {
                if let Some(status) = process.poll().unwrap() {
                    break status;
                }
                assert!(std::time::Instant::now() < until);
                std::thread::sleep(Duration::from_millis(10));
            };
            assert_eq!(status.code(), Some(99));
            assert_eq!(
                fs::read(temp.path().join("model-started")).unwrap(),
                b"harmless fixture"
            );
            heartbeat.disarm_worker();
            let lease = heartbeat.stop().unwrap();
            owner.release(&lease).unwrap();
        } else {
            let error = match result {
                Err(error) => error,
                Ok(_) => panic!("late hook spawned a child after its original deadline"),
            };
            assert!(
                format!("{error:#}").contains("execution budget exhausted during final admission"),
                "{error:#}"
            );
            assert!(!temp.path().join("model-started").exists());
            assert!(
                !config
                    .state_dir
                    .join("runs")
                    .join(&job.run_id)
                    .join("process")
                    .exists()
            );
            assert_eq!(job.launch.as_ref().unwrap().pid, 0);
            admission::launch_failed(&config, &owner, &mut job, &mut writes, &error, true).unwrap();
            assert!(job.retry_after.is_none());
        }
        let observed = events(&temp);
        assert_eq!(
            observed
                .iter()
                .filter(|e| *e == "acquire-checked-v1")
                .count(),
            1
        );
        assert_eq!(observed.iter().filter(|e| *e == "claim").count(), 1);
        assert_eq!(observed.iter().filter(|e| *e == "release").count(), 1);
    }
}
