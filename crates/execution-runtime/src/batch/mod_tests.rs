use super::*;
use crate::job_store::job_path;
use crate::{
    git::git,
    jobs::{enqueue_with_inputs, prepare_retry},
    prompts::rendered_job_prompt,
};
use anyhow::bail;
use contracts::WorkflowState;
use native_effects::fs;
use serde_json::{Value, json};
use std::os::unix::fs::PermissionsExt;
use std::{collections::BTreeMap, path::PathBuf};
use task::Job;

struct Fixture {
    _temp: tempfile::TempDir,
    config: Config,
    inputs: inputs::ExecutionInputs,
    request: Request,
    control: PathBuf,
    calls: PathBuf,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        OBSERVER.with(|observer| *observer.borrow_mut() = None);
    }
}
impl Fixture {
    fn new(count: usize) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let repository = root.join("repository");
        fs::create_dir(&repository).unwrap();
        git(&repository, &["init", "-q"]).unwrap();
        git(&repository, &["config", "user.name", "fixture"]).unwrap();
        git(
            &repository,
            &["config", "user.email", "fixture@example.invalid"],
        )
        .unwrap();
        fs::write(repository.join("source"), "owned source\n").unwrap();
        git(&repository, &["add", "source"]).unwrap();
        git(
            &repository,
            &["-c", "commit.gpgsign=false", "commit", "-qm", "fixture"],
        )
        .unwrap();
        let commit = git(&repository, &["rev-parse", "HEAD"]).unwrap();
        let binary =
            PathBuf::from(std::env::var("LAB_WORKFLOW_BIN").expect("set real workflow CLI path"));
        let control = root.join("control.json");
        let calls = root.join("calls.jsonl");
        let wrapper = root.join("workflow-observer");
        fs::write(&wrapper, format!(r##"#!/usr/bin/python3
import hashlib,json,pathlib,subprocess,sys
control=pathlib.Path({control})
args=sys.argv[1:]
with open({calls},'a') as stream: stream.write(json.dumps(args)+'\n')
mode=json.loads(control.read_text()) if control.exists() else {{}}
operation=args[:2]
if operation==['run','start'] and mode.get('mode')=='reject_start':
 print(json.dumps({{'ok':False,'error':{{'code':'invalid_request','message':'injected rejection'}}}}));sys.exit(1)
if operation==['run','verify'] and mode.get('mode')=='unsupported_verify':
 print('unsupported',file=sys.stderr);sys.exit(2)
if operation==['run','verify'] and mode.get('mode')=='corrupt_verify':
 print('{{');sys.exit(0)
if operation==['run','verify'] and mode.get('mode')=='oversized_verify':
 print('x'*2097153);sys.exit(0)
result=subprocess.run([{binary}]+args,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
if operation==['run','start'] and mode.get('mode')=='lose_start':
 print('lost reply',file=sys.stderr);sys.exit(1)
if operation==['run','verify'] and result.returncode!=0 and mode.get('mode')=='live_on_absence':
 path=pathlib.Path(mode['job']);job=json.loads(path.read_text());job[mode['field']]=mode['value'];path.write_text(json.dumps(job))
def corrupt_state(snapshot):
 if mode.get('mode')=='unknown_state':snapshot['status']='future_state'
 if mode.get('mode')=='unknown_node':snapshot['frames']['1']['nodes']['task']['state']['state']='future_state'
if operation==['run','verify'] and result.returncode==0 and mode.get('mode') in ['unknown_state','unknown_node']:
 value=json.loads(result.stdout)
 historical=subprocess.run([{binary},'run','history-at',args[2],args[3],str(value['result']['revision'])],stdout=subprocess.PIPE,check=True)
 snapshot=json.loads(historical.stdout)['result'];corrupt_state(snapshot)
 value['result']['state_digest']='sha256:'+hashlib.sha256(json.dumps(snapshot,sort_keys=True,ensure_ascii=False,separators=(',',':')).encode()).hexdigest()
 result.stdout=json.dumps(value,ensure_ascii=False).encode()
if operation==['run','history-at'] and result.returncode==0:
 value=json.loads(result.stdout);corrupt_state(value['result'])
 if mode.get('mode')=='bad_history': value['result']['revision']=999999
 if mode.get('mode')=='attempt_race':
  path=pathlib.Path(mode['job']);job=json.loads(path.read_text());job['attempt']=3;job['run_id']=job['task']['id']+'-attempt-3';path.write_text(json.dumps(job))
 result.stdout=json.dumps(value,ensure_ascii=False).encode()
sys.stdout.buffer.write(result.stdout);sys.stderr.buffer.write(result.stderr);sys.exit(result.returncode)
"##, control=serde_json::to_string(&control).unwrap(), calls=serde_json::to_string(&calls).unwrap(), binary=serde_json::to_string(&binary).unwrap())).unwrap();
        fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
        let config: Config = serde_json::from_value(json!({
            "schema_version":1,"workspace":root,"state_dir":root.join("state"),"superpod":repository,
            "codex":"never-launched","workflow":wrapper,"daily_seconds":3600,"max_agents":8,
            "task_timeout_seconds":20,"models":{"research":"fixture","review":"fixture","implement":"fixture"},
            "tools":{},"require_latest":true,"skills_manifest":root.join("fixture-acquisition.json")
        })).unwrap();
        let inputs: inputs::ExecutionInputs = serde_json::from_value(json!({
            "repositories":{"superpod":{"upstream":{"repository":"fixture/owned","default_branch":"main","commit":commit,"checked_at":"old"},"checkout":repository}},
            "skills":{"schema_version":1,"skills":[]},"insights":"checked_at literal original", "insights_sha256":storage::digest(b"checked_at literal original")
        })).unwrap();
        let request = Request { schema_version:1, batch_id:"fixture-batch".into(), tasks:(0..count).map(|i| serde_json::from_value(json!({
            "id":format!("task-{i}"),"role":"research","repository":"superpod","prompt":format!("inspect owned fixture {i}"),
            "write":false,"exploratory":i%2==0,"max_attempts":2,"depth":1,"use_memory":false
        })).unwrap()).collect() };
        Self {
            _temp: temp,
            config,
            inputs,
            request,
            control,
            calls,
        }
    }
    fn persona(&mut self) {
        let binary = self._temp.path().join("memory-fixture");
        let counter = self._temp.path().join("memory-calls");
        fs::write(&binary,format!(r#"#!/usr/bin/python3
import json,pathlib
p=pathlib.Path({path});n=int(p.read_text())+1 if p.exists() else 1;p.write_text(str(n));print(json.dumps({{'bootstrap':'memory nonce '+str(n)}}))
"#,path=serde_json::to_string(&counter).unwrap())).unwrap();
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
        self.config.tools.insert(
            "relay-memory".into(),
            config::Tool {
                binary,
                repository: self.config.superpod.clone(),
                probe: vec![],
            },
        );
        let id = people::default_id(&self.config, "research").unwrap();
        for task in &mut self.request.tasks {
            task.persona_id = Some(id.clone());
            task.use_memory = true;
        }
    }
    fn memory_calls(&self) -> usize {
        fs::read_to_string(self._temp.path().join("memory-calls"))
            .unwrap_or_else(|_| "0".into())
            .parse()
            .unwrap()
    }
    fn select(&self) -> String {
        let mut registry = evolution::PromptRegistry::default();
        let version = registry
            .register(
                evolution::PromptVersion::new(evolution::PromptDefinition {
                    parent_versions: vec![],
                    role: "research".into(),
                    task_kind: "fixture".into(),
                    content: "registered fixture role".into(),
                    change_reason: "test immutable selection".into(),
                    failure_conditions: vec!["binding changed".into()],
                })
                .unwrap(),
            )
            .unwrap();
        storage::write(
            &self.config.state_dir.join("evolution/prompts.json"),
            &registry,
        )
        .unwrap();
        storage::write(
            &self.config.state_dir.join("evolution/selections.json"),
            &evolution_cli::StrategySelections {
                schema_version: 1,
                roles: BTreeMap::from([(
                    "research".into(),
                    evolution_cli::SelectedStrategy {
                        stable_version: version.clone(),
                        task_kind: "fixture".into(),
                        previous_stable_versions: vec![],
                        evidence_digest: "fixture".into(),
                        policy_version: "fixture".into(),
                    },
                )]),
            },
        )
        .unwrap();
        version
    }
    fn run(&self) -> Result<Receipt> {
        self.run_inputs(self.inputs.clone())
    }
    fn run_inputs(&self, inputs: inputs::ExecutionInputs) -> Result<Receipt> {
        enqueue_collected(
            &self.config,
            self.request.clone(),
            digest(&self.request)?,
            60,
            || Ok(inputs),
        )
    }
    fn mode(&self, mode: &str) {
        storage::write(&self.control, &json!({"mode":mode})).unwrap();
    }
    fn commands(&self) -> Vec<Vec<String>> {
        fs::read_to_string(&self.calls)
            .unwrap_or_default()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }
    fn writes(&self) -> usize {
        self.commands()
            .iter()
            .filter(|args| {
                args.first().map(String::as_str) == Some("run")
                    && !["verify", "history-at", "status", "storage-plan"]
                        .contains(&args[1].as_str())
            })
            .count()
    }
    fn job(&self, index: usize) -> Job {
        storage::read(&job_path(&self.config, &self.request.tasks[index].id)).unwrap()
    }
    fn bytes(&self, index: usize) -> Vec<u8> {
        fs::read(job_path(&self.config, &self.request.tasks[index].id)).unwrap()
    }
    fn plan(&self) -> PathBuf {
        self.config
            .state_dir
            .join("private/enqueue-batches/fixture-batch/plan.json")
    }
    fn assert_no_registration(&self) {
        assert!(!self.config.state_dir.join("jobs").exists());
        assert!(!self.config.state_dir.join("worktrees").exists());
        assert!(!self.config.state_dir.join("people/directory.json").exists());
        assert_eq!(self.writes(), 0);
    }
}
fn observed(error: &anyhow::Error) -> &Receipt {
    error
        .downcast_ref::<Failure>()
        .expect("typed failure")
        .registration
        .as_deref()
        .expect("partial receipt")
}
fn hook(callback: impl FnMut(Phase, &str) -> Result<()> + 'static) {
    OBSERVER.with(|observer| *observer.borrow_mut() = Some(Box::new(callback)));
}

#[test]
#[ignore = "requires LAB_WORKFLOW_BIN; real isolated owner, acquisition fixture only, no model"]
fn eight_native_registrations_refresh_once_and_replay_without_effects() {
    let mut f = Fixture::new(8);
    f.config.tools.insert(
        "workflow-cli".into(),
        config::Tool {
            binary: f.config.workflow.clone(),
            repository: f.config.superpod.clone(),
            probe: vec![],
        },
    );
    f.request.tasks[2].role = "implement".into();
    f.request.tasks[2].write = true;
    f.request.tasks[3].role = "review".into();
    f.request.tasks[3].dependencies = vec!["task-0".into()];
    f.request.tasks[4].required_tools = vec!["workflow-cli".into()];
    f.request.tasks[4].use_memory = true;
    let mut collections = 0;
    let result = enqueue_collected(&f.config, f.request.clone(), "raw-file".into(), 60, || {
        collections += 1;
        Ok(f.inputs.clone())
    })
    .unwrap();
    assert_eq!(collections, 1);
    assert_eq!(result.state, State::Complete);
    assert!(
        result
            .entries
            .iter()
            .all(|entry| entry.state == EntryState::WorkflowConfirmed)
    );
    let before: Vec<_> = (0..8).map(|i| f.bytes(i)).collect();
    let plan = fs::read(f.plan()).unwrap();
    let writes = f.writes();
    let mut fresh = f.inputs.clone();
    fresh
        .repositories
        .get_mut("superpod")
        .unwrap()
        .upstream
        .checked_at = "new".into();
    assert_eq!(f.run_inputs(fresh).unwrap().state, State::Complete);
    assert_eq!(f.writes(), writes);
    assert_eq!(fs::read(f.plan()).unwrap(), plan);
    for (i, bytes) in before.iter().enumerate() {
        assert_eq!(&f.bytes(i), bytes);
        assert_eq!(
            digest(&f.job(i).task).unwrap(),
            digest(&f.request.tasks[i]).unwrap()
        );
        assert_eq!(
            workflow(&f.config).status(&f.job(i).run_id).unwrap()["status"],
            "running"
        );
    }
    assert!(!f.config.state_dir.join("runs").exists());
    assert!(f.commands().iter().all(|args| {
        !args
            .iter()
            .any(|a| ["claim", "drive", "acquire", "drive-models"].contains(&a.as_str()))
    }));
}

#[test]
#[ignore = "requires LAB_WORKFLOW_BIN; real isolated owner, no model"]
fn late_conflicts_leave_no_registration_prefix_and_no_directory_save() {
    let mut f = Fixture::new(8);
    f.request.tasks[7].dependencies = vec!["missing".into()];
    assert!(f.run().is_err());
    f.assert_no_registration();
    f.request.tasks[7].dependencies.clear();
    let id = format!("person-{}", &storage::digest(b"default:research")[..24]);
    f.request.tasks[0].persona_id = Some(id);
    let mut prior =
        registration::prepare_frozen(&f.config, f.request.tasks[7].clone(), Some(&f.inputs), None)
            .unwrap()
            .job(&f.config, Some(f.inputs.clone()))
            .unwrap();
    prior.task.prompt.push_str(" conflicting");
    storage::write(&job_path(&f.config, &prior.task.id), &prior).unwrap();
    let before = fs::read(job_path(&f.config, &prior.task.id)).unwrap();
    assert!(f.run().is_err());
    assert_eq!(f.writes(), 0);
    assert!(!f.config.state_dir.join("worktrees").exists());
    assert!(!f.config.state_dir.join("people").exists());
    assert_eq!(
        fs::read(job_path(&f.config, &prior.task.id)).unwrap(),
        before
    );
}

#[test]
#[ignore = "requires LAB_WORKFLOW_BIN; real isolated owner, no model"]
fn fresh_failure_drift_and_canonical_request_conflict_preserve_authority() {
    let mut f = Fixture::new(1);
    f.run().unwrap();
    let original_job = f.bytes(0);
    let original_plan = fs::read(f.plan()).unwrap();
    let writes = f.writes();
    let error = enqueue_collected(
        &f.config,
        f.request.clone(),
        "new-raw-format".into(),
        60,
        || bail!("fresh acquisition failed"),
    )
    .unwrap_err();
    assert_eq!(observed(&error).error_code, Some(Code::FreshnessFailure));
    for mutate in 0..5 {
        let mut fresh = f.inputs.clone();
        match mutate {
            0 => fresh
                .repositories
                .get_mut("superpod")
                .unwrap()
                .checkout
                .push("changed"),
            1 => {
                fresh
                    .repositories
                    .get_mut("superpod")
                    .unwrap()
                    .upstream
                    .default_branch = "other".into()
            }
            2 => {
                fresh
                    .repositories
                    .get_mut("superpod")
                    .unwrap()
                    .upstream
                    .repository = "other/source".into()
            }
            3 => fresh.insights.push_str("checked_at changed"),
            _ => fresh.insights_sha256 = "different".into(),
        }
        assert_eq!(
            observed(&f.run_inputs(fresh).unwrap_err()).error_code,
            Some(Code::FreshnessDrift)
        );
    }
    assert_eq!(
        enqueue_collected(
            &f.config,
            f.request.clone(),
            "whitespace-different-raw".into(),
            60,
            || Ok(f.inputs.clone())
        )
        .unwrap()
        .state,
        State::Complete
    );
    let receipt_path = f.plan().with_file_name("receipt.json");
    let old_receipt = fs::read(&receipt_path).unwrap();
    f.request.tasks[0].prompt.push_str("different");
    assert!(f.run().is_err());
    assert_eq!(fs::read(receipt_path).unwrap(), old_receipt);
    assert_eq!(f.bytes(0), original_job);
    assert_eq!(fs::read(f.plan()).unwrap(), original_plan);
    assert_eq!(f.writes(), writes);
}

#[test]
#[ignore = "requires LAB_WORKFLOW_BIN; real isolated owner, no model"]
fn rejected_and_lost_start_have_distinct_receipts_and_replay_is_idempotent() {
    for mode in ["reject_start", "lose_start"] {
        let f = Fixture::new(2);
        f.mode(mode);
        let error = f.run().unwrap_err();
        assert_eq!(
            observed(&error).entries[0].state,
            if mode == "reject_start" {
                EntryState::JobRecorded
            } else {
                EntryState::Unknown
            }
        );
        assert_eq!(observed(&error).entries[1].state, EntryState::Pending);
        let original = f.bytes(0);
        let stable = workflow(&f.config).work_dir.join(format!(
            "start-{}.json",
            storage::digest(f.job(0).run_id.as_bytes())
        ));
        let start = fs::read(&stable).unwrap();
        f.mode("none");
        assert_eq!(f.run().unwrap().state, State::Complete);
        assert_eq!(f.bytes(0), original);
        assert_eq!(fs::read(stable).unwrap(), start);
        let history = workflow(&f.config).verify(&f.job(0).run_id).unwrap();
        assert_eq!(history["revision"], 1);
    }
}

#[test]
#[ignore = "requires LAB_WORKFLOW_BIN; real isolated owner, no model"]
fn filesystem_failures_report_exact_stage_and_receipt_persistence() {
    for stage in [
        Phase::RegisterJob,
        Phase::BindPersona,
        Phase::PersistReceipt,
    ] {
        let mut f = Fixture::new(2);
        if stage == Phase::BindPersona {
            f.persona();
        }
        let config = f.config.clone();
        let root = f.plan().parent().unwrap().to_path_buf();
        let mut count = 0;
        hook(move |phase, id| {
            if id == "task-0" && phase == stage {
                count += 1;
                if stage == Phase::RegisterJob && count == 2 {
                    fs::create_dir_all(job_path(&config, id))?;
                }
                if stage == Phase::BindPersona {
                    fs::create_dir_all(config.state_dir.join("people"))?;
                    fs::write(config.state_dir.join("people/directory.json"), b"invalid")?;
                }
                if stage == Phase::PersistReceipt {
                    fs::remove_file(root.join("receipt.json"))?;
                    fs::create_dir(root.join("receipt.json"))?;
                }
            }
            Ok(())
        });
        let error = f.run().unwrap_err();
        let receipt = observed(&error);
        assert_eq!(
            receipt.entries[0].state,
            match stage {
                Phase::RegisterJob => EntryState::Pending,
                Phase::BindPersona => EntryState::JobRecorded,
                _ => EntryState::WorkflowConfirmed,
            }
        );
        assert!(!job_path(&f.config, "task-1").exists());
        if stage == Phase::PersistReceipt {
            assert!(!receipt.receipt_persisted);
            assert_eq!(receipt.error_code, Some(Code::ReceiptPersistenceFailure));
        }
    }
}

#[test]
#[ignore = "requires LAB_WORKFLOW_BIN; real isolated owner, no model"]
fn current_advanced_attempt_requires_exact_readonly_owner_proof() {
    let f = Fixture::new(1);
    f.run().unwrap();
    let mut job = f.job(0);
    job.attempt = 2;
    job.run_id = "task-0-attempt-2".into();
    storage::write(&job_path(&f.config, &job.task.id), &job).unwrap();
    let bytes = f.bytes(0);
    let writes = f.writes();
    assert!(f.run().is_err());
    assert_eq!(f.writes(), writes);
    assert_eq!(f.bytes(0), bytes);
    job.attempt = 1;
    job.run_id = "task-0-attempt-1".into();
    prepare_retry(&f.config, &mut job).unwrap();
    let writes = f.writes();
    let receipt = f.run().unwrap();
    assert_eq!(
        receipt.entries[0].state,
        EntryState::AdvancedExistingConfirmed
    );
    assert_eq!(f.writes(), writes);
    assert_eq!(f.bytes(0), bytes);
    for mode in [
        "unsupported_verify",
        "corrupt_verify",
        "bad_history",
        "oversized_verify",
        "unknown_state",
        "unknown_node",
    ] {
        f.mode(mode);
        let error = f.run().unwrap_err();
        assert_eq!(observed(&error).entries[0].state, EntryState::Unknown);
        assert_eq!(f.writes(), writes);
        assert_eq!(f.bytes(0), bytes);
    }
    storage::write(
        &f.control,
        &json!({"mode":"attempt_race","job":job_path(&f.config,&job.task.id)}),
    )
    .unwrap();
    let error = f.run().unwrap_err();
    assert_eq!(
        observed(&error).error_code,
        Some(Code::CurrentAttemptChanged)
    );
    assert_eq!(f.writes(), writes);
    assert_eq!(f.job(0).attempt, 3);
}

#[test]
#[ignore = "requires LAB_WORKFLOW_BIN; real isolated owner, no model"]
fn deadline_bounds_and_nonblocking_registration_locks() {
    let f = Fixture::new(2);
    let root = f.plan().parent().unwrap().to_path_buf();
    let lock = storage::lock(&root.join("batch.lock")).unwrap();
    assert!(f.run().is_err());
    f.assert_no_registration();
    drop(lock);
    let lock = storage::lock(&f.config.state_dir.join("enqueue.lock")).unwrap();
    assert!(f.run().is_err());
    f.assert_no_registration();
    drop(lock);
    let error = enqueue_collected(&f.config, f.request.clone(), "raw".into(), 1, || {
        std::thread::sleep(Duration::from_millis(1100));
        Ok(f.inputs.clone())
    })
    .unwrap_err();
    assert_eq!(observed(&error).error_code, Some(Code::DeadlineExceeded));
    f.assert_no_registration();
    for count in [0, 33] {
        let mut request = f.request.clone();
        request.tasks = vec![request.tasks[0].clone(); count];
        assert!(
            enqueue_collected(&f.config, request, "raw".into(), 60, || panic!(
                "invalid count collected"
            ))
            .is_err()
        );
    }
    for seconds in [0, 601] {
        assert!(
            enqueue_collected(
                &f.config,
                f.request.clone(),
                "raw".into(),
                seconds,
                || panic!("invalid deadline collected")
            )
            .is_err()
        );
    }
}

#[test]
#[ignore = "requires LAB_WORKFLOW_BIN; real owner and explicit memory acquisition fixture"]
fn frozen_builtin_persona_and_registered_prompt_replay_do_not_reselect_or_recapture() {
    let mut f = Fixture::new(2);
    f.persona();
    f.mode("reject_start");
    assert!(f.run().is_err());
    assert_eq!(f.memory_calls(), 2);
    let old = f.bytes(0);
    let plan = fs::read(f.plan()).unwrap();
    f.select();
    f.mode("none");
    let id = f.request.tasks[0].persona_id.clone().unwrap();
    let person = people::directory(&f.config).unwrap().people[&id].clone();
    people::change(
        &f.config,
        people::Change::Update {
            id,
            revision: person.revision,
            name: person.name,
            soul: "changed current profile".into(),
            purpose: person.purpose,
        },
    )
    .unwrap();
    assert_eq!(f.run().unwrap().state, State::Complete);
    assert_eq!(f.memory_calls(), 2);
    assert_eq!(f.bytes(0), old);
    assert_eq!(fs::read(f.plan()).unwrap(), plan);
    assert!(f.job(1).task.prompt_version.is_none());
    assert_ne!(f.job(1).persona.unwrap().soul, "changed current profile");
    let g = Fixture::new(1);
    let version = g.select();
    g.run().unwrap();
    assert_eq!(
        g.job(0).task.prompt_version.as_deref(),
        Some(version.as_str())
    );
    let mut registry: Value =
        storage::read(&g.config.state_dir.join("evolution/prompts.json")).unwrap();
    registry["versions"][&version]["definition"]["content"] = json!("modified immutable prompt");
    storage::write(
        &g.config.state_dir.join("evolution/prompts.json"),
        &registry,
    )
    .unwrap();
    assert!(g.run().is_err());
    fs::remove_file(g.config.state_dir.join("evolution/prompts.json")).unwrap();
    assert!(g.run().is_err());
}

#[test]
#[ignore = "requires LAB_WORKFLOW_BIN; real owner, no model"]
fn whole_lock_barriers_revalidate_races_and_never_repair_an_early_prefix() {
    for compatible in [true, false] {
        let f = Fixture::new(2);
        let c = f.config.clone();
        let inputs = f.inputs.clone();
        let task = f.request.tasks[1].clone();
        let mut injected = false;
        hook(move |phase, id| {
            if phase == Phase::Prepare && id.is_empty() && !injected {
                injected = true;
                let mut task = task.clone();
                if !compatible {
                    task.prompt.push_str(" conflict");
                }
                enqueue_with_inputs(&c, task, Some(inputs.clone()))?;
            }
            Ok(())
        });
        let result = f.run();
        if compatible {
            assert_eq!(result.unwrap().state, State::Complete);
        } else {
            assert!(result.is_err());
            assert!(!job_path(&f.config, "task-0").exists());
            assert!(!f.config.state_dir.join("worktrees/task-0").exists());
        }
    }
    let f = Fixture::new(8);
    let first =
        registration::prepare_frozen(&f.config, f.request.tasks[0].clone(), Some(&f.inputs), None)
            .unwrap()
            .job(&f.config, Some(f.inputs.clone()))
            .unwrap();
    storage::write(&job_path(&f.config, "task-0"), &first).unwrap();
    let mut last =
        registration::prepare_frozen(&f.config, f.request.tasks[7].clone(), Some(&f.inputs), None)
            .unwrap()
            .job(&f.config, Some(f.inputs.clone()))
            .unwrap();
    last.source_commit = "e".repeat(40);
    storage::write(&job_path(&f.config, "task-7"), &last).unwrap();
    let before = f.bytes(0);
    assert!(f.run().is_err());
    assert_eq!(f.bytes(0), before);
    assert_eq!(f.writes(), 0);
    assert!(!f.config.state_dir.join("worktrees").exists());
    let f = Fixture::new(2);
    let c = f.config.clone();
    let inputs = f.inputs.clone();
    let mut contender = f.request.tasks[0].clone();
    contender.id = "contender".into();
    hook(move |phase, id| {
        if phase == Phase::Preflight && id.is_empty() {
            assert!(enqueue_with_inputs(&c, contender.clone(), Some(inputs.clone())).is_err());
        }
        Ok(())
    });
    assert_eq!(f.run().unwrap().state, State::Complete);
    assert!(!job_path(&f.config, "contender").exists());
}

#[test]
#[ignore = "requires LAB_WORKFLOW_BIN; real owner, no model"]
fn late_advanced_wrong_payload_blocks_missing_prefix() {
    let f = Fixture::new(8);
    let mut job =
        registration::prepare_frozen(&f.config, f.request.tasks[7].clone(), Some(&f.inputs), None)
            .unwrap()
            .job(&f.config, Some(f.inputs.clone()))
            .unwrap();
    job.attempt = 2;
    job.run_id = "task-7-attempt-2".into();
    storage::write(&job_path(&f.config, &job.task.id), &job).unwrap();
    let w = workflow(&f.config);
    w.initialize().unwrap();
    w.start_task(
        &job.run_id,
        false,
        json!({"wrong":"frozen payload"}),
        f.config.task_timeout_seconds * 1000,
    )
    .unwrap();
    let writes = f.writes();
    let bytes = f.bytes(7);
    let error = f.run().unwrap_err();
    assert_eq!(observed(&error).entries[7].state, EntryState::Unknown);
    assert_eq!(f.writes(), writes);
    assert_eq!(f.bytes(7), bytes);
    assert!(!job_path(&f.config, "task-0").exists());
    assert!(!f.config.state_dir.join("worktrees").exists());
}

#[test]
#[ignore = "requires LAB_WORKFLOW_BIN; real owner, no model"]
fn ordinary_replay_and_existing_persona_adoption_keep_original_bytes() {
    let mut f = Fixture::new(1);
    f.persona();
    enqueue_with_inputs(
        &f.config,
        f.request.tasks[0].clone(),
        Some(f.inputs.clone()),
    )
    .unwrap();
    let job = f.bytes(0);
    let calls = f.memory_calls();
    assert_eq!(f.run().unwrap().state, State::Complete);
    assert_eq!(f.memory_calls(), calls);
    assert_eq!(f.bytes(0), job);
    let mut ordinary = f.config.clone();
    ordinary.task_timeout_seconds += 10;
    crate::enqueue(&ordinary, f.request.tasks[0].clone()).unwrap();
    assert_eq!(f.memory_calls(), calls);
    assert_eq!(f.bytes(0), job);
    assert!(
        enqueue_collected(&ordinary, f.request.clone(), "raw".into(), 60, || Ok(f
            .inputs
            .clone()))
        .is_err()
    );
    let mut explicit = f.request.tasks[0].clone();
    explicit.prompt_version = Some("different".into());
    assert!(crate::enqueue(&f.config, explicit).is_err());
    assert_eq!(f.bytes(0), job);
}

#[test]
#[ignore = "requires LAB_WORKFLOW_BIN; real owner, no model"]
fn serialized_plan_and_actual_request_bounds_precede_registration() {
    let f = Fixture::new(1);
    let path = f._temp.path().join("request.json");
    let raw = serde_json::to_vec(&f.request).unwrap();
    let mut padded = raw.clone();
    padded.resize(plan::REQUEST_LIMIT, b' ');
    fs::write(&path, &padded).unwrap();
    assert!(plan::read_request(&path).is_ok());
    padded.push(b' ');
    fs::write(&path, padded).unwrap();
    assert!(enqueue(&f.config, &path, 60).is_err());
    f.assert_no_registration();
    let mut request = f.request.clone();
    request.tasks[0].prompt = "long prompt".repeat(100);
    let mut huge = f.inputs.clone();
    huge.insights = "\n".repeat(8 * 1024 * 1024);
    let error = enqueue_collected(&f.config, request, "raw".into(), 60, || Ok(huge)).unwrap_err();
    assert!(format!("{error:#}").contains("16 MiB"));
    f.assert_no_registration();
    assert!(!f.plan().exists());
}

#[test]
#[ignore = "requires LAB_WORKFLOW_BIN; actual missing owner plus same-attempt Job race"]
fn same_attempt_progress_after_owner_absence_is_unknown_with_zero_repair() {
    for (field, value) in [
        ("retry_after", json!(123)),
        ("last_error", json!("concurrent retry")),
        (
            "launch",
            json!({
                "pid":1,"process_start":"fixture","lease":{},"attempt":{},"log_dir":"/owned-fixture","tools":{},"git_dir":""
            }),
        ),
    ] {
        let f = Fixture::new(1);
        let job = registration::prepare_frozen(
            &f.config,
            f.request.tasks[0].clone(),
            Some(&f.inputs),
            None,
        )
        .unwrap()
        .job(&f.config, Some(f.inputs.clone()))
        .unwrap();
        storage::write(&job_path(&f.config, &job.task.id), &job).unwrap();
        workflow(&f.config).initialize().unwrap();
        let writes = f.writes();
        storage::write(&f.control,&json!({"mode":"live_on_absence","job":job_path(&f.config,&job.task.id),"field":field,"value":value})).unwrap();
        let error = f.run().unwrap_err();
        let receipt = observed(&error);
        assert_eq!(receipt.entries[0].state, EntryState::Unknown);
        assert_eq!(receipt.state, State::Incomplete);
        assert_eq!(receipt.error_code, Some(Code::OwnerProofUnavailable));
        assert_eq!(f.writes(), writes);
        assert!(!f.config.state_dir.join("worktrees").exists());
        assert!(!f.config.state_dir.join("people").exists());
        assert_eq!(serde_json::to_value(f.job(0)).unwrap()[field], value);
    }
}

#[test]
#[ignore = "requires LAB_WORKFLOW_BIN; exact post-proof settlement race"]
fn settled_proof_does_not_confirm_a_later_attempt() {
    let f = Fixture::new(1);
    f.run().unwrap();
    let path = job_path(&f.config, "task-0");
    let writes = f.writes();
    hook(move |phase, id| {
        if phase == Phase::PersistPlan && id.is_empty() {
            let mut job: Job = storage::read(&path)?;
            job.attempt = 2;
            job.run_id = "task-0-attempt-2".into();
            storage::write(&path, &job)?;
        }
        Ok(())
    });
    let error = f.run().unwrap_err();
    let receipt = observed(&error);
    assert_eq!(receipt.state, State::Incomplete);
    assert_eq!(receipt.error_code, Some(Code::CurrentAttemptChanged));
    assert_eq!(receipt.entries[0].state, EntryState::Unknown);
    assert!(receipt.entries[0].proof.is_some());
    assert_eq!(f.writes(), writes);
    assert_eq!(f.job(0).attempt, 2);
}

#[test]
#[ignore = "requires LAB_WORKFLOW_BIN; missing current Job after owner proof"]
fn settled_proof_requires_a_readable_current_job_missing() {
    unreadable_job_after_proof(false);
}

#[test]
#[ignore = "requires LAB_WORKFLOW_BIN; corrupt current Job after owner proof"]
fn settled_proof_requires_a_readable_current_job_corrupt() {
    unreadable_job_after_proof(true);
}

fn unreadable_job_after_proof(corrupt: bool) {
    let f = Fixture::new(1);
    f.run().unwrap();
    let path = job_path(&f.config, "task-0");
    let changed = path.clone();
    let writes = f.writes();
    hook(move |phase, id| {
        if phase == Phase::PersistPlan && id.is_empty() {
            if corrupt {
                fs::write(&changed, "{invalid Job")?;
            } else {
                fs::remove_file(&changed)?;
            }
        }
        Ok(())
    });
    let error = f.run().unwrap_err();
    let receipt = observed(&error);
    assert_eq!(receipt.state, State::Incomplete);
    assert_eq!(receipt.entries[0].state, EntryState::Unknown);
    assert!(receipt.entries[0].proof.is_some());
    assert_eq!(f.writes(), writes);
    if corrupt {
        assert_eq!(fs::read_to_string(&path).unwrap(), "{invalid Job");
    } else {
        assert!(!path.exists());
    }
}

#[test]
#[ignore = "requires LAB_WORKFLOW_BIN; actual interrupted eighth worktree"]
fn eighth_interrupted_wrong_worktree_rejects_before_any_prefix_effect() {
    let f = Fixture::new(8);
    fs::write(f.config.superpod.join("new"), "second commit\n").unwrap();
    git(&f.config.superpod, &["add", "new"]).unwrap();
    git(
        &f.config.superpod,
        &["-c", "commit.gpgsign=false", "commit", "-qm", "second"],
    )
    .unwrap();
    let head = git(&f.config.superpod, &["rev-parse", "HEAD"]).unwrap();
    let worktree = f.config.state_dir.join("worktrees/task-7");
    fs::create_dir_all(worktree.parent().unwrap()).unwrap();
    git(
        &f.config.superpod,
        &[
            "worktree",
            "add",
            "--detach",
            worktree.to_str().unwrap(),
            &head,
        ],
    )
    .unwrap();
    let error = f.run().unwrap_err();
    assert_eq!(
        observed(&error).error_code,
        Some(Code::FrozenBindingConflict)
    );
    assert!(!f.config.state_dir.join("jobs").exists());
    assert!(!f.config.state_dir.join("worktrees/task-0").exists());
    assert!(!f.config.state_dir.join("people").exists());
    assert_eq!(f.writes(), 0);
    assert_eq!(git(&worktree, &["rev-parse", "HEAD"]).unwrap(), head);
}

#[test]
#[ignore = "requires LAB_WORKFLOW_BIN; real owner upper count and cumulative deadline"]
fn thirty_two_members_and_deadline_after_prefix_preserve_exact_progress() {
    let f = Fixture::new(32);
    assert_eq!(f.run().unwrap().entries.len(), 32);
    let f = Fixture::new(3);
    hook(|phase, id| {
        if phase == Phase::PersistReceipt && id == "task-0" {
            std::thread::sleep(Duration::from_millis(1100));
        }
        Ok(())
    });
    let error = enqueue_collected(&f.config, f.request.clone(), "raw".into(), 1, || {
        Ok(f.inputs.clone())
    })
    .unwrap_err();
    assert_eq!(observed(&error).error_code, Some(Code::DeadlineExceeded));
    assert_eq!(
        observed(&error).entries[0].state,
        EntryState::WorkflowConfirmed
    );
    assert_eq!(observed(&error).entries[1].state, EntryState::Pending);
    assert!(job_path(&f.config, "task-0").exists());
    assert!(!job_path(&f.config, "task-1").exists());
}

#[test]
#[ignore = "requires LAB_WORKFLOW_BIN; real owner terminal run fixture, batch remains read-only"]
fn failed_and_cancelled_current_runs_confirm_registration_without_claiming_success() {
    for cancelled in [false, true] {
        let f = Fixture::new(1);
        f.run().unwrap();
        let mut job = f.job(0);
        prepare_retry(&f.config, &mut job).unwrap();
        let w = workflow(&f.config);
        if cancelled {
            let snapshot = w.status(&job.run_id).unwrap();
            let output = process::capture(
                f.config.workflow.to_str().unwrap(),
                &[
                    "run".into(),
                    "cancel".into(),
                    w.database.to_string_lossy().into_owned(),
                    job.run_id.clone(),
                    "cancel-fixture".into(),
                    snapshot["revision"].as_u64().unwrap().to_string(),
                    snapshot["now_unix_ms"].as_u64().unwrap().to_string(),
                ],
                &w.work_dir,
                Duration::from_secs(10),
            )
            .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stdout)
            );
            let lease = w
                .acquire(&job.run_id, "cancel-fixture", "cancel-acquisition", 30000)
                .unwrap();
            for _ in 0..4 {
                if w.status(&job.run_id).unwrap()["status"] == "cancelled" {
                    break;
                }
                w.claim(&lease).unwrap();
            }
            w.release(&lease).unwrap();
        } else {
            let lease = w
                .acquire(&job.run_id, "owned-fixture", "owned-acquisition", 30000)
                .unwrap();
            let claimed = w.claim(&lease).unwrap();
            w.fail(
                &lease,
                &claimed["attempt"],
                "explicit fixture failure; no process or model run",
            )
            .unwrap();
            w.release(&lease).unwrap();
        }
        let before = f.bytes(0);
        let writes = f.writes();
        let receipt = f.run().unwrap();
        assert_eq!(
            receipt.entries[0].state,
            EntryState::AdvancedExistingConfirmed
        );
        assert_eq!(
            receipt.entries[0].proof.as_ref().unwrap().state,
            if cancelled {
                WorkflowState::Cancelled
            } else {
                WorkflowState::Failed
            }
        );
        assert_eq!(f.writes(), writes);
        assert_eq!(f.bytes(0), before);
    }
}

#[test]
#[ignore = "requires LAB_WORKFLOW_BIN; actual owner proof shares outer deadline"]
fn owner_proof_deadline_never_claims_confirmation() {
    let f = Fixture::new(1);
    f.run().unwrap();
    let writes = f.writes();
    let old = f.bytes(0);
    let _deadline = process::deadline_scope(Duration::from_millis(1));
    std::thread::sleep(Duration::from_millis(3));
    let mut observation = Observation {
        task_id: "task-0".into(),
        state: EntryState::Pending,
        attempt: None,
        run_id: None,
        worktree: None,
        proof: None,
        frozen_input_digest: None,
    };
    assert!(observe(&f.config, &f.job(0), &mut observation).is_err());
    assert_eq!(observation.state, EntryState::Unknown);
    assert!(observation.proof.is_none());
    assert_eq!(f.writes(), writes);
    assert_eq!(f.bytes(0), old);
}

#[test]
#[ignore = "requires LAB_WORKFLOW_BIN; real native task preparation, no model"]
fn new_write_review_and_forward_existing_dependencies_reject_without_prefix() {
    let mut f = Fixture::new(2);
    f.request.tasks[0].role = "implement".into();
    f.request.tasks[0].write = true;
    f.request.tasks[1].role = "review".into();
    f.request.tasks[1].dependencies = vec!["task-0".into()];
    assert!(f.run().is_err());
    f.assert_no_registration();
    let mut f = Fixture::new(2);
    enqueue_with_inputs(
        &f.config,
        f.request.tasks[1].clone(),
        Some(f.inputs.clone()),
    )
    .unwrap();
    f.request.tasks[0].dependencies = vec!["task-1".into()];
    let writes = f.writes();
    let old = f.bytes(1);
    assert!(f.run().is_err());
    assert_eq!(f.writes(), writes);
    assert_eq!(f.bytes(1), old);
    assert!(!job_path(&f.config, "task-0").exists());
}

#[test]
#[ignore = "requires LAB_WORKFLOW_BIN; exact immutable binding settlement race"]
fn settled_owner_proof_does_not_cover_rewritten_input_timestamps() {
    let f = Fixture::new(1);
    f.run().unwrap();
    let c = f.config.clone();
    let writes = f.writes();
    hook(move |phase, id| {
        if phase == Phase::PersistPlan && id.is_empty() {
            let path = job_path(&c, "task-0");
            let mut job: Job = storage::read(&path)?;
            job.research_inputs
                .as_mut()
                .unwrap()
                .repositories
                .get_mut("superpod")
                .unwrap()
                .upstream
                .checked_at = "changed after proof".into();
            job.prompt_digest = storage::digest(rendered_job_prompt(&c, &job)?.as_bytes());
            storage::write(&path, &job)?;
        }
        Ok(())
    });
    let error = f.run().unwrap_err();
    let receipt = observed(&error);
    assert_eq!(receipt.entries[0].state, EntryState::Unknown);
    assert_eq!(receipt.error_code, Some(Code::FrozenBindingConflict));
    assert_eq!(f.writes(), writes);
}

#[test]
#[ignore = "requires LAB_WORKFLOW_BIN; real owned Git write receipt and native effect owner"]
fn existing_committed_candidate_preserves_baseline_and_independent_person_check() {
    let mut f = Fixture::new(1);
    f.persona();
    let mut implementation = f.request.tasks[0].clone();
    implementation.id = "parent-implementation".into();
    implementation.role = "implement".into();
    implementation.write = true;
    let parent = enqueue_with_inputs(&f.config, implementation, Some(f.inputs.clone())).unwrap();
    fs::write(
        parent.worktree.join("candidate"),
        "actual owned candidate edit\n",
    )
    .unwrap();
    git(&parent.worktree, &["add", "candidate"]).unwrap();
    git(
        &parent.worktree,
        &[
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-qm",
            "owned candidate",
        ],
    )
    .unwrap();
    let candidate = git(&parent.worktree, &["rev-parse", "HEAD"]).unwrap();
    let receipt = json!({"candidate":{"candidate_commit":candidate,"baseline_commit":parent.source_commit},"fixture":"host performed the owned Git write; no model or research-quality claim"});
    let receipt_path = f
        .config
        .state_dir
        .join("runs")
        .join(&parent.run_id)
        .join("receipt.json");
    storage::write(&receipt_path, &receipt).unwrap();
    let w = workflow(&f.config);
    let lease = w
        .acquire(&parent.run_id, "fixture-writer", "fixture-write", 30000)
        .unwrap();
    let claimed = w.effect_claim(&lease).unwrap();
    w.finish_effect(
        &lease,
        &claimed["attempt"],
        &candidate,
        &receipt_path,
        json!({"result":serde_json::to_string(&receipt).unwrap()}),
    )
    .unwrap();
    w.release(&lease).unwrap();
    assert_eq!(w.status(&parent.run_id).unwrap()["status"], "succeeded");
    f.request.tasks[0].role = "review".into();
    f.request.tasks[0].dependencies = vec![parent.task.id.clone()];
    let before = f.writes();
    assert!(f.run().is_err());
    assert!(!job_path(&f.config, "task-0").exists());
    assert_eq!(f.writes(), before);
    f.request.tasks[0].persona_id = Some(people::default_id(&f.config, "review").unwrap());
    assert_eq!(f.run().unwrap().state, State::Complete);
    let review = f.job(0);
    assert_eq!(review.source_commit, candidate);
    assert_eq!(review.baseline_commit.as_ref(), Some(&parent.source_commit));
    assert_ne!(review.persona.unwrap().id, parent.persona.unwrap().id);
}
