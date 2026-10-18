use crate::admission;
use crate::completion::{ExitObservation, settle_with_heartbeat};
use crate::git::git;
use crate::host_budget::{Ledger, now};
use crate::job_store::{job_path, jobs, workflow};
use crate::jobs::{enqueue, enqueue_with_inputs, retry};
use crate::launch::{launch, launch_guarded};
use crate::people;
use crate::post_success::{drain_followups_scoped, post_success_path};
use crate::prompts::rendered_job_prompt;
use crate::scheduler::{run, run_scoped, run_scoped_impl, run_scoped_with_observations};
use crate::seeding::seed_inputs;
use crate::tests;
use crate::{followups::FollowupReport, receipts::authoritative_success_receipt};
use anyhow::{Result, bail, ensure};
use config::Config;
use contracts::{FollowupAdmission, MemoryWriteback, WorkflowState};
use multi_agent::TeamMembership;
use native_effects::fs;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Duration;
use task::{Job, Launch, PostSuccess, Task};
use workflow::Workflow;

pub(super) fn completion_fixture(
    memory: bool,
    proposals: Value,
) -> (tempfile::TempDir, Config, Job, Workflow) {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let repo = root.join("superpod");
    fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-b", "main"]).unwrap();
    fs::write(repo.join("README.md"), "fixture\n").unwrap();
    git(&repo, &["add", "README.md"]).unwrap();
    git(
        &repo,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-m",
            "fixture",
        ],
    )
    .unwrap();
    let commit = git(&repo, &["rev-parse", "HEAD"]).unwrap();
    let binary = root.join("workflow-fixture");
    fs::write(&binary, format!(
            "#!/bin/sh\nif [ \"$1\" = run ] && [ \"$2\" = status ]; then cat '{}'; else printf '%s\\n' '{{\"ok\":true,\"result\":{{}}}}'; fi\n",
            root.join("authority.json").display()
        )).unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
    let memory_binary = root.join("memory-fixture");
    fs::write(
        &memory_binary,
        format!(
            "#!/bin/sh\nprintf x >> '{}'\nprintf '%s\\n' '{{\"remembered\":true}}'\n",
            root.join("memory-calls").display()
        ),
    )
    .unwrap();
    fs::set_permissions(&memory_binary, fs::Permissions::from_mode(0o700)).unwrap();
    let c: Config = serde_json::from_value(json!({
        "schema_version":1,"workspace":root,"state_dir":root.join("state"),
        "superpod":repo,"codex":"unused","workflow":binary,
        "daily_seconds":3600,"max_agents":1,"task_timeout_seconds":20,
        "models":{"research":"fixture","review":"fixture","implement":"fixture"},
        "tools":{"relay-memory":{"binary":memory_binary,"repository":repo,"probe":[]}},
        "require_latest":false,"skills_manifest":null
    }))
    .unwrap();
    let job: Job = serde_json::from_value(json!({
            "task":{"id":"synthesis-recovery","role":"research","repository":"superpod","prompt":"frozen","use_memory":memory},
            "model":"fixture","source_commit":commit,"superpod_commit":commit,
            "prompt_digest":"prompt","config_digest":"config","worktree":repo,
            "run_id":"synthesis-recovery-attempt-1","attempt":1,"last_error":null,"launch":null
        })).unwrap();
    let receipt = json!({"receipt_protocol":2,"config_digest":job.config_digest,"agent_report":{"summary":"observed fixture result","findings":[],"sources":[],"limitations":["fake transport only"],"next_tasks":proposals},
            "source_commit":job.source_commit,"superpod_commit":job.superpod_commit,"prompt_digest":job.prompt_digest,
            "model":job.model,"research_inputs":null,"post_success_protocol":1});
    storage::write(&root.join("authority.json"), &json!({"ok":true,"result":{"status":"succeeded","frames":{"1":{"nodes":{"task":{"outputs":{"result":serde_json::to_string(&receipt).unwrap()}}}}}}})).unwrap();
    storage::write(&job_path(&c, &job.task.id), &job).unwrap();
    let w = workflow(&c);
    fs::create_dir_all(&w.work_dir).unwrap();
    (temp, c, job, w)
}

#[cfg(unix)]
pub(super) fn scheduler_fixture() -> (tempfile::TempDir, Config) {
    use std::collections::BTreeMap;
    use std::os::unix::fs::PermissionsExt;
    let binary = std::env::var("LAB_WORKFLOW_BIN").expect("LAB_WORKFLOW_BIN");
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("superpod");
    fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-b", "main"]).unwrap();
    fs::write(repo.join("README.md"), "fixture repository\n").unwrap();
    git(&repo, &["add", "README.md"]).unwrap();
    git(
        &repo,
        &[
            "-c",
            "user.name=Lab Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-m",
            "fixture",
        ],
    )
    .unwrap();
    let fake = temp.path().join("fake-codex");
    fs::write(&fake,r#"#!/bin/sh
output=''
sandbox=''
while [ "$#" -gt 0 ]; do
  case "$1" in
    -o) shift; output="$1" ;;
    -s) shift; sandbox="$1" ;;
  esac
  shift
done
prompt=$(cat)
case "$prompt" in *SLOW_PARALLEL*) date +%s%N; sleep 3; date +%s%N ;; esac
case "$prompt" in *EIGHT_PARALLEL*) date +%s%N; sleep 20; date +%s%N ;; *AFTER_EIGHT*) date +%s%N; date +%s%N ;; esac
case "$prompt" in *FAIL_READ*) exit 7 ;; esac
case "$prompt" in *implement\ tested\ candidate*|*MALFORMED*) printf 'actual fixture write' > candidate.txt ;; esac
case "$prompt" in
  *MALFORMED*) printf '{"summary":"malformed"}' > "$output" ;;
  *PROPOSE_FOLLOWUP*) printf '{"summary":"fixture proposal","findings":[],"sources":[],"limitations":["fake only"],"next_tasks":[{"repository":"superpod","role":"review","prompt":"bounded independent review","write":false,"exploratory":false}]}' > "$output" ;;
  *BAD_PROPOSAL*) printf '{"summary":"invalid proposal","findings":[],"sources":[],"limitations":["fake only"],"next_tasks":[{"repository":"../../secrets","role":"implement","prompt":"invalid","write":true,"exploratory":false}]}' > "$output" ;;
  *) printf '{"summary":"fixture output","findings":["fake provider only"],"sources":[],"limitations":["not a research result"]}' > "$output" ;;
esac
printf '{"type":"fixture.completed"}\n'
"#).unwrap();
    fs::set_permissions(&fake, fs::Permissions::from_mode(0o700)).unwrap();
    let tools = [
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
                repository: repo.clone(),
                probe: vec![],
            },
        )
    })
    .collect();
    let mut c = Config {
        require_latest: false,
        skills_manifest: None,
        schema_version: 1,
        workspace: temp.path().into(),
        state_dir: temp.path().join("state"),
        superpod: repo,
        codex: fake.to_str().unwrap().into(),
        multi_agent: None,
        agent_backends: BTreeMap::new(),
        role_backends: BTreeMap::new(),
        backend_models: BTreeMap::new(),
        workflow: binary.into(),
        daily_seconds: 3600,
        max_agents: 2,
        task_timeout_seconds: 20,
        models: BTreeMap::from([
            ("research".into(), "fake-research".into()),
            ("review".into(), "fake-review".into()),
            ("implement".into(), "fake-implement".into()),
        ]),
        tools,
    };
    if let Ok(binary) = std::env::var("LAB_MEMORY_BIN") {
        c.tools.get_mut("relay-memory").unwrap().binary = binary.into();
    }
    (temp, c)
}

#[cfg(unix)]
#[test]
#[ignore = "real workflow/bwrap with explicit fake JSON bridge; set LAB_WORKFLOW_BIN"]
fn actual_workflow_json_bridge_contract_and_process_lifetime() {
    use std::os::unix::fs::PermissionsExt;
    let (temp, mut c) = scheduler_fixture();
    let bridge = temp.path().join("fake-json-bridge");
    fs::write(&bridge, r#"#!/usr/bin/python3
import hashlib, json, os, pathlib, sys, time
raw = sys.stdin.buffer.read()
r = json.loads(raw)
assert r['schema_version'] == 1
assert 'OPENAI_API_KEY' not in os.environ and 'CODEX_HOME' not in os.environ
out = pathlib.Path(r['result_path'])
mode = r['request_id'].split('-attempt-')[0]
report = {'summary':'explicit fake bridge only','findings':[],'sources':[],'limitations':['protocol fixture, not model research'],'next_tasks':[]}
if r['permissions']['worktree_write']:
    pathlib.Path(r['worktree'], 'candidate.txt').write_text('fixture bridge candidate')
if r.get('bindings', {}).get('probe'): report = {'ok':True}
if mode in ['timeout', 'pause', 'detached-exit']:
    if os.fork() == 0:
        os.setsid()
        while True:
            with open(out.parent / 'child-counter', 'a') as f: f.write('x')
            time.sleep(0.03)
    while not (out.parent / 'child-counter').exists(): time.sleep(0.01)
    if mode != 'detached-exit': time.sleep(30)
if mode == 'malformed-write': out.write_text('{'); sys.exit(0)
if mode == 'missing': sys.exit(0)
if mode == 'fifo': os.mkfifo(out); sys.exit(0)
if mode == 'oversize': out.write_text(' ' * (1024*1024+1)); sys.exit(0)
digest = hashlib.sha256(raw).hexdigest()
if mode == 'request-tamper':
    (out.parent / 'bridge-request.json').write_text('modified')
    digest = hashlib.sha256(b'modified').hexdigest()
out.write_text(json.dumps({'schema_version':1,'request_sha256':digest,'report':report}))
"#).unwrap();
    fs::set_permissions(&bridge, fs::Permissions::from_mode(0o700)).unwrap();
    let spec = agent_backend::BackendSpec::JsonProcess {
        program: bridge,
        args: vec![],
        env_allowlist: vec![],
        capabilities: agent_backend::Capabilities {
            structured_result: true,
            read_workspace: true,
            write_workspace: true,
            tool_execution: true,
            desktop: false,
        },
    };
    c.agent_backends.insert("fixture-bridge".into(), spec);
    for role in ["research", "implement", "review"] {
        c.role_backends.insert(role.into(), "fixture-bridge".into());
    }
    let probe = agent_backend::probe(&c, "review", &c.models["review"]).unwrap();
    assert_eq!(probe["probe"], "isolated_json_contract");
    assert_eq!(probe["provider_compatibility"], "unverified");
    let w = workflow(&c);
    for mode in [
        "success",
        "write-success",
        "malformed-write",
        "missing",
        "fifo",
        "oversize",
        "request-tamper",
        "detached-exit",
        "timeout",
        "pause",
    ] {
        let write = mode.contains("write");
        let task:Task=serde_json::from_value(json!({"id":mode,"role":if write {"implement"} else {"research"},"repository":"superpod","prompt":"Run the explicit protocol fixture only.","write":write,"max_attempts":1})).unwrap();
        let mut job = enqueue(&c, task).unwrap();
        let (mut worker, mut heartbeat) =
            launch(&c, &w, &mut job, if mode == "timeout" { 2 } else { 15 }).unwrap();
        let logs = c.state_dir.join("runs").join(&job.run_id);
        let frozen: Job = storage::read(&job_path(&c, mode)).unwrap();
        assert!(
            frozen.launch.unwrap().backend_request_sha256.is_some(),
            "host must freeze digest before spawn"
        );
        let started = std::time::Instant::now();
        let observation = loop {
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "backend process failed to terminate: {mode}"
            );
            if mode == "pause" && logs.join("child-counter").exists() {
                fs::write(c.state_dir.join("paused"), "fixture pause").unwrap();
            }
            match worker.poll() {
                Ok(Some(exit)) => {
                    break ExitObservation {
                        success: exit.success(),
                        code: exit.code(),
                        reason: None,
                    };
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(30)),
                Err(error) => {
                    break ExitObservation {
                        success: false,
                        code: None,
                        reason: Some(error.to_string()),
                    };
                }
            }
        };
        // Resume only this private fixture; production state is never used.
        let _ = fs::remove_file(c.state_dir.join("paused"));
        job.launch.as_mut().unwrap().lease = heartbeat.stop().unwrap();
        let accepted = settle_with_heartbeat(&w, &job, &observation, None).unwrap();
        let expected = matches!(mode, "success" | "write-success" | "detached-exit");
        assert_eq!(accepted, expected, "{mode}");
        assert_eq!(
            WorkflowState::from_value(&w.status(&job.run_id).unwrap()["status"])
                == Some(WorkflowState::Succeeded),
            expected,
            "{mode}"
        );
        w.verify(&job.run_id).unwrap();
        if expected {
            let receipt = authoritative_success_receipt(&w, &job).unwrap();
            assert_eq!(
                receipt["backend"],
                serde_json::to_value(&job.backend).unwrap()
            );
            assert_eq!(
                receipt["backend_request_sha256"],
                json!(job.launch.as_ref().unwrap().backend_request_sha256)
            );
        }
        if logs.join("child-counter").exists() {
            let before = fs::read(logs.join("child-counter")).unwrap();
            std::thread::sleep(Duration::from_millis(150));
            assert_eq!(
                before,
                fs::read(logs.join("child-counter")).unwrap(),
                "detached child survived: {mode}"
            );
        }
        assert_eq!(job.attempt, 1);
    }
}

#[cfg(unix)]
#[test]
#[ignore = "real workflow, bwrap and relay-memory; set LAB_WORKFLOW_BIN and LAB_MEMORY_BIN; fake providers only"]
fn digital_person_uses_different_executors_without_rebinding_history_or_memory() {
    use std::os::unix::fs::PermissionsExt;
    let (temp, mut c) = scheduler_fixture();
    assert!(std::env::var_os("LAB_MEMORY_BIN").is_some());
    let bridge = temp.path().join("person-bridge");
    fs::write(&bridge, r#"#!/usr/bin/python3
import hashlib, json, pathlib, sys
raw = sys.stdin.buffer.read()
r = json.loads(raw)
assert r['model'] == 'bridge-model'
assert r['bindings']['digital_person']['id'].startswith('person-')
assert r['bindings']['digital_person']['session_id'] == 'person-second'
assert r['bindings']['digital_person']['execution_id'] == r['request_id']
assert 'PERSON_EXECUTION_CONTINUITY_2741' in r['prompt']
assert not r['permissions']['worktree_write']
report = {'summary':'explicit executor continuity fixture','findings':['same person across technical executors'],'sources':[],'limitations':['fake provider, not research quality'],'next_tasks':[]}
pathlib.Path(r['result_path']).write_text(json.dumps({'schema_version':1,'request_sha256':hashlib.sha256(raw).hexdigest(),'report':report}))
print(json.dumps({'type':'session.activity','kind':'message','text':'The same persona continues research.'}))
"#).unwrap();
    fs::set_permissions(&bridge, fs::Permissions::from_mode(0o700)).unwrap();
    c.agent_backends.insert(
        "person-bridge".into(),
        config::BackendSpec::JsonProcess {
            program: bridge,
            args: vec![],
            env_allowlist: vec![],
            capabilities: config::Capabilities {
                structured_result: true,
                read_workspace: true,
                ..Default::default()
            },
        },
    );
    c.backend_models
        .insert("person-bridge".into(), vec!["bridge-model".into()]);
    let person = people::change(
        &c,
        people::Change::Create {
            name: "InsightAction".into(),
            role: "designer".into(),
            kind: people::Kind::Fixed,
            soul: "Respond to peers with curiosity and care.".into(),
            purpose: "Research continuity across execution technologies".into(),
        },
    )
    .unwrap();
    people::remember_note(
        &c,
        &person.id,
        "continuity-note",
        "PERSON_EXECUTION_CONTINUITY_2741: Preserve identity and memory across executors.",
    )
    .unwrap();
    let task = |id: &str, role: &str| {
        serde_json::from_value::<Task>(json!({"id":id,"role":role,"repository":"superpod","prompt":"PERSON_EXECUTION_CONTINUITY_2741 Continue researching cross-technology persona memory continuity.","persona_id":person.id,"max_attempts":1})).unwrap()
    };
    let first_task = task("person-first", "research");
    let first = enqueue(&c, first_task.clone()).unwrap();
    let original = serde_json::to_value(&first).unwrap();
    people::change(
        &c,
        people::Change::ConfigureExecution {
            id: person.id.clone(),
            revision: person.revision,
            execution: Some(task::ExecutionPreference {
                backend: Some("person-bridge".into()),
                model: Some("bridge-model".into()),
            }),
        },
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(enqueue(&c, first_task).unwrap()).unwrap(),
        original
    );
    let second = enqueue(&c, task("person-second", "review")).unwrap();
    assert!(first.backend.is_none());
    assert_eq!(first.model, "fake-research");
    assert_eq!(second.backend.as_ref().unwrap().id, "person-bridge");
    assert_eq!(second.model, "bridge-model");
    for job in [&first, &second] {
        assert_eq!(job.persona.as_ref().unwrap().id, person.id);
        assert!(
            job.persona
                .as_ref()
                .unwrap()
                .memory
                .as_ref()
                .unwrap()
                .context
                .contains("PERSON_EXECUTION_CONTINUITY_2741")
        );
        assert!(!job.task.write);
        agent_backend::verify_job(&c, job).unwrap();
    }
    let mut forbidden = second.task.clone();
    forbidden.write = true;
    assert!(agent_backend::freeze_selected(&c, &forbidden, Some("person-bridge")).is_err());
    let mut changed = second.clone();
    changed.model = "fake-research".into();
    assert!(agent_backend::verify_job(&c, &changed).is_err());
    let w = workflow(&c);
    for mut job in [first, second] {
        let (mut worker, mut heartbeat) = launch(&c, &w, &mut job, 15).unwrap();
        let start = std::time::Instant::now();
        let exit = loop {
            assert!(start.elapsed() < Duration::from_secs(20));
            if let Some(exit) = worker.poll().unwrap() {
                break exit;
            }
            std::thread::sleep(Duration::from_millis(30));
        };
        job.launch.as_mut().unwrap().lease = heartbeat.stop().unwrap();
        assert!(
            settle_with_heartbeat(
                &w,
                &job,
                &ExitObservation {
                    success: exit.success(),
                    code: exit.code(),
                    reason: None
                },
                None
            )
            .unwrap()
        );
        let receipt = authoritative_success_receipt(&w, &job).unwrap();
        people::writeback(&c, &job, &receipt).unwrap();
        w.verify(&job.run_id).unwrap();
    }
    let directory = people::directory(&c).unwrap();
    assert_eq!(directory.tasks["person-first"], person.id);
    assert_eq!(directory.tasks["person-second"], person.id);
    let memory =
        people::inspect_memory(&c, &person.id, "PERSON_EXECUTION_CONTINUITY_2741").unwrap();
    assert_eq!(memory["stats"]["event_count"], 3);
    people::change(
        &c,
        people::Change::ConfigureExecution {
            id: person.id.clone(),
            revision: directory.people[&person.id].revision,
            execution: None,
        },
    )
    .unwrap();
    let mut author = task("person-author", "implement");
    author.write = true;
    enqueue(&c, author).unwrap();
    let mut self_review = task("person-self-review", "review");
    self_review.dependencies = vec!["person-author".into()];
    assert!(
        enqueue(&c, self_review)
            .unwrap_err()
            .to_string()
            .contains("different digital person")
    );
}

#[cfg(unix)]
#[test]
#[ignore = "real workflow and bwrap with explicit fake Codex; set LAB_WORKFLOW_BIN"]
fn scoped_run_preserves_historical_followups_and_executes_only_selected_dag() {
    let (_temp, mut c) = scheduler_fixture();
    let task = |id: &str, prompt: &str, dependencies: Vec<String>| Task {
        persona_id: None,
        id: id.into(),
        role: "research".into(),
        repository: "superpod".into(),
        prompt: prompt.into(),
        prompt_version: None,
        communication: None,
        max_attempts: Some(1),
        use_memory: false,
        depth: 0,
        write: false,
        exploratory: false,
        dependencies,
        required_tools: vec![],
    };
    let old = enqueue(&c, task("synthesis-history", "PROPOSE_FOLLOWUP", vec![])).unwrap();
    let history_scope = c.state_dir.join("private/history.json");
    storage::write(&history_scope, &vec![old.task.id.clone()]).unwrap();
    let started = std::time::Instant::now();
    let historical_result = run_scoped(&c, false, 30, false, Some(&history_scope)).unwrap();
    assert!(
        started.elapsed() < Duration::from_secs(25),
        "pending scope-deferred proposal must not spin until budget exhaustion"
    );
    assert_eq!(
        historical_result["task_scope"]["deferred_followup_runs"],
        json!([old.run_id])
    );
    let original_journal = fs::read(post_success_path(&c, &old)).unwrap();
    let historical_record: PostSuccess = serde_json::from_slice(&original_journal).unwrap();
    assert_eq!(historical_record.followups, FollowupAdmission::Pending);
    assert_eq!(jobs(&c).unwrap().len(), 1);
    let outside = enqueue(
        &c,
        task("outside-pending", "independent excluded task", vec![]),
    )
    .unwrap();
    c.task_timeout_seconds += 1; // A new pilot config must not consume the old cohort's proposals.
    let first = enqueue(&c, task("selected-first", "bounded evidence", vec![])).unwrap();
    let second = enqueue(
        &c,
        task(
            "selected-second",
            "combine supplied evidence",
            vec![first.task.id.clone()],
        ),
    )
    .unwrap();
    let proposal = enqueue(&c, task("synthesis-selected", "PROPOSE_FOLLOWUP", vec![])).unwrap();
    let scope_file = c.state_dir.join("private/selected.json");
    storage::write(
        &scope_file,
        &vec![
            first.task.id.clone(),
            second.task.id.clone(),
            proposal.task.id.clone(),
        ],
    )
    .unwrap();
    // An unrelated outbox marker must remain untouched by this bounded run.
    let outbox = c.state_dir.join("outbox/unrelated.json");
    storage::write(&outbox, &json!({"private_fixture":"do not process"})).unwrap();
    let outbox_before = fs::read(&outbox).unwrap();
    let started = std::time::Instant::now();
    let result = run_scoped(&c, false, 30, false, Some(&scope_file)).unwrap();
    assert!(started.elapsed() < Duration::from_secs(25));
    let w = workflow(&c);
    for job in [&first, &second, &proposal] {
        assert_eq!(w.status(&job.run_id).unwrap()["status"], "succeeded");
        w.verify(&job.run_id).unwrap();
    }
    assert_eq!(
        fs::read(post_success_path(&c, &old)).unwrap(),
        original_journal
    );
    assert_eq!(fs::read(outbox).unwrap(), outbox_before);
    assert!(
        !c.state_dir.join("outbox/states").exists(),
        "scope must not initialize or consume unrelated automation"
    );
    assert!(!c.state_dir.join("runs").join(&outside.run_id).exists());
    assert_eq!(w.status(&outside.run_id).unwrap()["status"], "running");
    assert_eq!(
        jobs(&c).unwrap().len(),
        5,
        "no historical or selected follow-up may be admitted"
    );
    let record: PostSuccess = storage::read(&post_success_path(&c, &proposal)).unwrap();
    assert_eq!(record.followups, FollowupAdmission::Pending);
    assert_eq!(
        result["task_scope"]["deferred_followup_runs"],
        json!([proposal.run_id])
    );
    let prompt = fs::read_to_string(
        c.state_dir
            .join("runs")
            .join(&second.run_id)
            .join("prompt.txt"),
    )
    .unwrap();
    assert!(prompt.contains("Completed dependency reports") && prompt.contains(&first.task.id));
    // Scope deferral is reversible without repairing or rewriting the journal.
    assert!(drain_followups_scoped(&c, &w, None, &mut BTreeSet::new()).unwrap());
    assert_eq!(jobs(&c).unwrap().len(), 6);
}

#[cfg(unix)]
#[test]
#[ignore = "set LAB_WORKFLOW_BIN for real durable workflow and bwrap with explicit fake Codex; optional LAB_PROTOCOL_EVIDENCE_DIR preserves receipts"]
fn eight_workers_respect_capacity_and_completed_dependency_barrier() {
    let (_temp, mut c) = scheduler_fixture();
    c.max_agents = 8;
    c.task_timeout_seconds = 45;
    let independent: Vec<String> = (0..10).map(|n| format!("parallel-{n:02}")).collect();
    let prerequisites = independent[..8].to_vec();
    for id in independent
        .iter()
        .chain(std::iter::once(&"after-eight".into()))
    {
        let dependent = id == "after-eight";
        enqueue(
            &c,
            Task {
                persona_id: None,
                id: id.clone(),
                role: if dependent { "review" } else { "research" }.into(),
                repository: "superpod".into(),
                prompt: if dependent {
                    "AFTER_EIGHT"
                } else {
                    "EIGHT_PARALLEL"
                }
                .into(),
                prompt_version: None,
                communication: None,
                max_attempts: None,
                use_memory: false,
                depth: 0,
                write: false,
                exploratory: false,
                dependencies: if dependent {
                    prerequisites.clone()
                } else {
                    vec![]
                },
                required_tools: vec![],
            },
        )
        .unwrap();
    }
    run(&c, false, 100).unwrap();
    let evidence = std::env::var_os("LAB_PROTOCOL_EVIDENCE_DIR").map(PathBuf::from);
    if let Some(path) = &evidence {
        fs::create_dir_all(path).unwrap();
    }
    let w = workflow(&c);
    let mut intervals = BTreeMap::new();
    let mut receipts = BTreeMap::new();
    for job in jobs(&c).unwrap() {
        assert_eq!(job.attempt, 1);
        assert!(job.last_error.is_none() && job.launch.is_none(), "{job:?}");
        assert_eq!(w.status(&job.run_id).unwrap()["status"], "succeeded");
        receipts.insert(job.task.id.clone(), w.verify(&job.run_id).unwrap());
        let dir = c.state_dir.join("runs").join(&job.run_id);
        let stdout = fs::read_to_string(dir.join("process/stdout.jsonl")).unwrap();
        let times: Vec<u128> = stdout.lines().filter_map(|s| s.parse().ok()).collect();
        assert_eq!(times.len(), 2, "{stdout}");
        intervals.insert(job.task.id.clone(), [times[0], times[1]]);
        if let Some(path) = &evidence {
            let task_dir = path.join(&job.task.id);
            fs::create_dir_all(&task_dir).unwrap();
            storage::write(&task_dir.join("job.json"), &job).unwrap();
            for file in [
                "prompt.txt",
                "result.json",
                "agent-permissions.json",
                "process/stdout.jsonl",
                "process/stderr.log",
            ] {
                let source = dir.join(file);
                if source.is_file() {
                    std::fs::copy(&source, task_dir.join(source.file_name().unwrap())).unwrap();
                }
            }
        }
    }
    let mut events = Vec::new();
    for (id, times) in &intervals {
        events.push((times[0], 1_i32, id));
        events.push((times[1], -1_i32, id));
    }
    events.sort();
    let mut active = 0;
    let mut peak = 0;
    for (_, change, _) in &events {
        active += change;
        peak = peak.max(active);
    }
    let last_prerequisite = prerequisites
        .iter()
        .map(|id| intervals[id][1])
        .max()
        .unwrap();
    let summary = json!({"provider":"explicit fake Codex; real workflow and bwrap", "max_agents":c.max_agents,"measured_peak":peak,"intervals_ns":intervals,"launch_and_finish_events_ns":events,"last_prerequisite_finish_ns":last_prerequisite,"workflow_verification":receipts,"limits":"Protocol concurrency and dependency evidence only; no model quality, token or cost result."});
    if let Some(path) = &evidence {
        storage::write(&path.join("protocol.json"), &summary).unwrap();
    }
    println!("{summary}");
    assert_eq!(
        peak, 8,
        "must fill all eight slots without admitting a ninth worker"
    );
    assert_eq!(active, 0);
    assert!(intervals["after-eight"][0] >= last_prerequisite);
    let dependent: Job = storage::read(&job_path(&c, "after-eight")).unwrap();
    let prompt = fs::read_to_string(
        c.state_dir
            .join("runs")
            .join(dependent.run_id)
            .join("prompt.txt"),
    )
    .unwrap();
    for id in prerequisites {
        assert!(
            prompt.contains(&id),
            "missing completed dependency report: {id}"
        );
    }
}

#[cfg(unix)]
#[test]
#[ignore = "set LAB_WORKFLOW_BIN and LAB_SANDBOX_BIN for real workflow/sandbox with explicit fake Codex"]
fn sandbox_write_claim_waits_for_idle_without_serializing_ordinary_workers() {
    let (_temp, mut c) = scheduler_fixture();
    c.max_agents = 3;
    c.tools.get_mut("repo-sandbox").unwrap().binary = std::env::var("LAB_SANDBOX_BIN")
        .expect("LAB_SANDBOX_BIN")
        .into();
    for (id, write) in [
        ("a-read", false),
        ("b-read", false),
        ("c-sandbox-write", true),
    ] {
        enqueue(
            &c,
            Task {
                persona_id: None,
                id: id.into(),
                role: if write { "implement" } else { "research" }.into(),
                repository: "superpod".into(),
                prompt: if write {
                    "implement tested candidate"
                } else {
                    "SLOW_PARALLEL"
                }
                .into(),
                prompt_version: None,
                communication: None,
                max_attempts: None,
                use_memory: false,
                depth: 0,
                write,
                exploratory: false,
                dependencies: vec![],
                required_tools: if write {
                    vec!["repo-sandbox".into()]
                } else {
                    vec![]
                },
            },
        )
        .unwrap();
    }
    run(&c, false, 30).unwrap();
    let w = workflow(&c);
    let mut intervals = vec![];
    for id in ["a-read", "b-read", "c-sandbox-write"] {
        let job: Job = storage::read(&job_path(&c, id)).unwrap();
        assert_eq!(job.attempt, 1, "preparation must not consume a task retry");
        assert!(job.last_error.is_none() && job.launch.is_none(), "{job:?}");
        assert_eq!(w.status(&job.run_id).unwrap()["status"], "succeeded");
        w.verify(&job.run_id).unwrap();
        if !job.task.write {
            let output = fs::read_to_string(
                c.state_dir
                    .join("runs")
                    .join(&job.run_id)
                    .join("process/stdout.jsonl"),
            )
            .unwrap();
            let times: Vec<u128> = output.lines().filter_map(|s| s.parse().ok()).collect();
            assert_eq!(times.len(), 2, "{output}");
            intervals.push(times);
        } else {
            let lease: Value = storage::read(
                &w.work_dir
                    .join("host-leases")
                    .join(format!("{}.json", job.run_id)),
            )
            .unwrap();
            let acquired = lease["issued_at_unix_ms"].as_u64().unwrap() as u128 * 1_000_000;
            assert!(
                intervals.iter().all(|times| acquired >= times[1]),
                "sandbox write was claimed while another worker was active"
            );
            let proof: Value = storage::read(
                &c.state_dir
                    .join("runs")
                    .join(&job.run_id)
                    .join("targets-host.json"),
            )
            .unwrap();
            assert_eq!(proof["verified"], true);
            assert_eq!(proof["down"]["status"], "stopped");
            assert!(job.worktree.join("candidate.txt").is_file());
        }
    }
    assert!(
        intervals[0][0] < intervals[1][1] && intervals[1][0] < intervals[0][1],
        "ordinary workers should still overlap"
    );
}

#[cfg(unix)]
#[test]
#[ignore = "set LAB_WORKFLOW_BIN to test the real durable runtime with an explicit fake Codex provider"]
fn actual_workflow_fake_codex_completion_and_failure() {
    let (_temp, c) = scheduler_fixture();
    let task = |id: &str, write: bool, prompt: &str, dependencies: Vec<String>| Task {
        persona_id: None,
        id: id.into(),
        role: if write {
            "implement".into()
        } else {
            "research".into()
        },
        repository: "superpod".into(),
        prompt: prompt.into(),
        prompt_version: None,
        communication: None,
        max_attempts: None,
        use_memory: false,
        depth: 0,
        write,
        exploratory: false,
        dependencies,
        required_tools: vec![],
    };
    enqueue(&c, task("research", false, "independent research", vec![])).unwrap();
    enqueue(&c, task("critic", false, "independent critique", vec![])).unwrap();
    enqueue(
        &c,
        task(
            "implementation",
            true,
            "implement tested candidate",
            vec!["research".into(), "critic".into()],
        ),
    )
    .unwrap();
    run(&c, false, 30).unwrap();
    let w = workflow(&c);
    for id in ["research", "critic", "implementation"] {
        let job: Job = storage::read(&job_path(&c, id)).unwrap();
        assert_eq!(w.status(&job.run_id).unwrap()["status"], "succeeded");
        assert!(job.launch.is_none());
        w.verify(&job.run_id).unwrap();
    }
    let prompt =
        fs::read_to_string(c.state_dir.join("runs/implementation-attempt-1/prompt.txt")).unwrap();
    assert!(prompt.contains("Completed dependency reports") && prompt.contains("fixture output"));
    let mut review = task(
        "candidate-review",
        false,
        "independently review frozen candidate",
        vec!["implementation".into()],
    );
    review.role = "review".into();
    let review = enqueue(&c, review).unwrap();
    assert!(review.baseline_commit.is_some());
    assert_ne!(
        review.source_commit,
        review.baseline_commit.as_ref().unwrap().as_str()
    );
    assert!(review.worktree.join("candidate.txt").exists());
    run(&c, false, 30).unwrap();
    assert_eq!(w.status(&review.run_id).unwrap()["status"], "succeeded");
    enqueue(&c, task("malformed-write", true, "MALFORMED", vec![])).unwrap();
    run(&c, false, 30).unwrap();
    let job: Job = storage::read(&job_path(&c, "malformed-write")).unwrap();
    assert!(job.last_error.is_some() && job.launch.is_none());
    assert_ne!(w.status(&job.run_id).unwrap()["status"], "succeeded");
    assert!(job.worktree.join("candidate.txt").exists());
    w.verify(&job.run_id).unwrap();
    enqueue(
        &c,
        task("synthesis-fixture", false, "PROPOSE_FOLLOWUP", vec![]),
    )
    .unwrap();
    enqueue(&c, task("synthesis-invalid", false, "BAD_PROPOSAL", vec![])).unwrap();
    run(&c, false, 30).unwrap();
    let parent: Job = storage::read(&job_path(&c, "synthesis-fixture")).unwrap();
    let followups: Value = storage::read(
        &c.state_dir
            .join("runs")
            .join(&parent.run_id)
            .join("followups.json"),
    )
    .unwrap();
    let child: Job =
        storage::read(&job_path(&c, followups["queued"][0].as_str().unwrap())).unwrap();
    assert_eq!(child.task.depth, 1);
    assert_eq!(w.status(&child.run_id).unwrap()["status"], "succeeded");
    let invalid: Job = storage::read(&job_path(&c, "synthesis-invalid")).unwrap();
    assert_eq!(w.status(&invalid.run_id).unwrap()["status"], "succeeded");
    let rejected: FollowupReport = storage::read(
        &c.state_dir
            .join("runs")
            .join(invalid.run_id)
            .join("followups.json"),
    )
    .unwrap();
    assert!(rejected.queued.is_empty());
    assert_eq!(rejected.rejected.len(), 1);
    if std::env::var("LAB_MEMORY_BIN").is_ok() {
        let version = evolution::PromptVersion::new(evolution::PromptDefinition {
            parent_versions: vec![],
            role: "research".into(),
            task_kind: "fixture".into(),
            content: "Registered alternate strategy".into(),
            change_reason: "fixture".into(),
            failure_conditions: vec!["missing evidence".into()],
        })
        .unwrap();
        let mut registry = evolution::PromptRegistry::default();
        let version = registry.register(version).unwrap();
        storage::write(&c.state_dir.join("evolution/prompts.json"), &registry).unwrap();
        for id in ["memory-baseline", "memory-candidate"] {
            let mut memory = task(id, false, "remember fixture constraints", vec![]);
            memory.use_memory = true;
            memory.prompt_version = Some(version.clone());
            enqueue(&c, memory).unwrap();
        }
        run(&c, false, 60).unwrap();
        for id in ["memory-baseline", "memory-candidate"] {
            let job: Job = storage::read(&job_path(&c, id)).unwrap();
            assert_eq!(
                w.status(&job.run_id).unwrap()["status"],
                "succeeded",
                "job: {job:?}"
            );
            let dir = c.state_dir.join("runs").join(&job.run_id);
            assert!(dir.join("memory-prepare.json").exists());
            assert!(dir.join("memory-remember.json").exists());
            assert!(
                c.state_dir
                    .join("task-memory")
                    .join(id)
                    .join("memory.sqlite")
                    .exists()
            );
            assert!(
                fs::read_to_string(dir.join("prompt.txt"))
                    .unwrap()
                    .contains("Registered alternate strategy")
            );
        }
    }
    let mut once = task("failed-once", false, "FAIL_READ", vec![]);
    once.max_attempts = Some(1);
    enqueue(&c, once).unwrap();
    enqueue(&c, task("failed-read", false, "FAIL_READ", vec![])).unwrap();
    run(&c, false, 30).unwrap();
    let job: Job = storage::read(&job_path(&c, "failed-read")).unwrap();
    assert_eq!(job.attempt, 3);
    assert!(job.retry_after.is_none());
    assert_eq!(w.status(&job.run_id).unwrap()["status"], "failed");
    let once: Job = storage::read(&job_path(&c, "failed-once")).unwrap();
    assert_eq!(once.attempt, 1);
    assert!(once.retry_after.is_none());
    assert_eq!(w.status(&once.run_id).unwrap()["status"], "failed");
    assert!(!c.state_dir.join("runs/failed-once-attempt-2").exists());
    assert!(retry(&c, "failed-once").is_err());
}

#[path = "team_tests.rs"]
mod team_tests;

#[path = "cohort_tests.rs"]
pub(crate) mod cohort_tests;

#[path = "admission_tests.rs"]
pub(super) mod admission_tests;

#[path = "filesystem_tests.rs"]
mod filesystem_tests;
