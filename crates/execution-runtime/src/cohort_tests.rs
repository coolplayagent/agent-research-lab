//! Scheduler regressions through run_scoped; only the external workflow is a fixture.
use super::*;
use std::os::unix::fs::PermissionsExt;

fn fixture() -> (tempfile::TempDir, Config) {
    let (temp, mut c, job, _) = tests::completion_fixture(false, json!([]));
    fs::remove_file(job_path(&c, &job.task.id)).unwrap();
    c.codex = "/bin/true".into();
    c.max_agents = 2;
    storage::write(&c.state_dir.join("budget.json"), &Ledger::default()).unwrap();
    c.tools = [
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
                repository: c.superpod.clone(),
                probe: vec![],
            },
        )
    })
    .collect();
    let script = r#"#!/usr/bin/python3
import json, pathlib, sys
root = pathlib.Path(__file__).parent
args = sys.argv[1:]
if args[0] == 'version':
    print(json.dumps({'version':'0.2.0'}))
    sys.exit(0)
if args[:2] == ['run','status']:
    states = json.loads((root / 'states.json').read_text())
    result = states.get(args[3], {'status':'running'})
elif args[:2] == ['run','acquire']:
    with (root / 'events').open('a') as out:
        out.write('acquire:' + json.loads(pathlib.Path(args[3]).read_text())['run_id'] + '\n')
    sys.exit(12)
else:
    result = {}
print(json.dumps({'ok':True,'result':result}))
"#;
    fs::write(&c.workflow, script).unwrap();
    fs::set_permissions(&c.workflow, fs::Permissions::from_mode(0o700)).unwrap();
    storage::write(&temp.path().join("states.json"), &json!({})).unwrap();
    (temp, c)
}

fn probe(c: &mut Config, name: &str, operation: &str) {
    let path = c.workspace.join(format!("probe-{name}"));
    fs::write(
        &path,
        format!(
            "#!/bin/sh\nprintf '%s\\n' 'probe:{name}' >> '{}'/events\n{operation}\n",
            c.workspace.display(),
        ),
    )
    .unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
    c.tools.insert(
        name.into(),
        config::Tool {
            binary: path,
            repository: c.superpod.clone(),
            probe: vec![],
        },
    );
}

fn add(c: &Config, id: &str, tool: &str, exploratory: bool) -> Job {
    enqueue(
        c,
        serde_json::from_value(json!({
            "id":id,"role":"research","repository":"superpod","prompt":"fixture",
            "use_memory":false,"max_attempts":1,"required_tools":[tool],
            "exploratory":exploratory
        }))
        .unwrap(),
    )
    .unwrap()
}

fn run_selected(c: &Config, ids: &[&str], seconds: u64) {
    let scope = c.state_dir.join("private/selected.json");
    storage::write(&scope, &ids).unwrap();
    run_scoped(c, false, seconds, false, Some(&scope)).unwrap();
}

fn events(c: &Config) -> Vec<String> {
    fs::read_to_string(c.workspace.join("events"))
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect()
}

#[test]
fn late_pause_after_failed_preparation_prevents_every_claim() {
    let (_temp, mut c) = fixture();
    probe(&mut c, "ready", "exit 0");
    probe(&mut c, "broken", "exit 7");
    let pause = format!("touch '{}'", c.state_dir.join("paused").display());
    probe(&mut c, "pause", &pause);
    add(&c, "a-ready", "ready", false);
    add(&c, "b-broken", "broken", false);
    add(&c, "c-pause", "pause", false);
    add(&c, "z-unselected", "ready", false);
    run_selected(&c, &["a-ready", "b-broken", "c-pause"], 20);
    assert_eq!(events(&c), ["probe:ready", "probe:broken", "probe:pause"]);
    for id in ["a-ready", "b-broken", "c-pause", "z-unselected"] {
        let job: Job = storage::read(&job_path(&c, id)).unwrap();
        assert!(job.launch.is_none());
        assert_eq!(job.attempt, 1);
        assert_eq!(job.last_error.is_some(), id == "b-broken");
    }
}

#[test]
fn capacity_bounds_preparation_and_launch_failure_keeps_next_track() {
    let (_temp, mut c) = fixture();
    probe(&mut c, "ready", "exit 0");
    for (id, exploratory) in [("a-main", false), ("b-main", false), ("c-other", true)] {
        add(&c, id, "ready", exploratory);
    }
    run_selected(&c, &["a-main", "b-main", "c-other"], 20);
    assert_eq!(
        events(&c),
        [
            "probe:ready",
            "probe:ready",
            "acquire:a-main-attempt-1",
            "acquire:b-main-attempt-1",
            "probe:ready",
            "acquire:c-other-attempt-1"
        ]
    );
}

#[test]
fn failed_main_preparation_allows_same_cohort_exploration() {
    let (_temp, mut c) = fixture();
    probe(&mut c, "broken", "exit 7");
    probe(&mut c, "ready", "exit 0");
    add(&c, "a-main", "broken", false);
    add(&c, "b-exploration", "ready", true);
    run_selected(&c, &["a-main", "b-exploration"], 20);
    assert_eq!(
        events(&c),
        [
            "probe:broken",
            "probe:ready",
            "acquire:b-exploration-attempt-1"
        ]
    );
}

#[test]
fn dependency_failures_unknown_workflows_and_pauses_never_prepare() {
    let (_temp, mut c) = fixture();
    probe(&mut c, "unexpected", "exit 0");
    let mut states = serde_json::Map::new();
    let mut ids = Vec::new();
    for (id, state) in [
        ("failed", json!({"status":WorkflowState::Failed})),
        ("cancelled", json!({"status":WorkflowState::Cancelled})),
        ("unknown", json!({"status":"future-state"})),
        (
            "paused",
            json!({"status":WorkflowState::Running,"pause":{}}),
        ),
    ] {
        let job = add(&c, id, "unexpected", false);
        states.insert(job.run_id, state);
        ids.push(id.to_string());
        let dependent = enqueue(
            &c,
            serde_json::from_value(json!({
                "id":format!("child-{id}"),"role":"research","repository":"superpod",
                "prompt":"dependent fixture","use_memory":false,"max_attempts":1,
                "required_tools":["unexpected"],"dependencies":[id]
            }))
            .unwrap(),
        )
        .unwrap();
        ids.push(dependent.task.id);
    }
    storage::write(&c.workspace.join("states.json"), &states).unwrap();
    let start = std::time::Instant::now();
    run_selected(&c, &ids.iter().map(String::as_str).collect::<Vec<_>>(), 20);
    assert!(events(&c).is_empty());
    assert!(
        start.elapsed() < Duration::from_secs(10),
        "blocked queues must return without spinning"
    );
}

#[test]
fn preparation_deadline_keeps_ready_tasks_unclaimed_and_charges_work() {
    let (_temp, mut c) = fixture();
    probe(&mut c, "ready", "exit 0");
    probe(&mut c, "slow", "sleep 5");
    add(&c, "a-ready", "ready", false);
    add(&c, "b-slow", "slow", false);
    let start = std::time::Instant::now();
    run_selected(&c, &["a-ready", "b-slow"], 2);
    assert!(start.elapsed() < Duration::from_secs(4));
    assert_eq!(events(&c), ["probe:ready", "probe:slow"]);
    for id in ["a-ready", "b-slow"] {
        let job: Job = storage::read(&job_path(&c, id)).unwrap();
        assert!(job.launch.is_none() && job.last_error.is_none());
        assert_eq!(job.attempt, 1);
    }
    let ledger: Ledger = storage::read(&c.state_dir.join("budget.json")).unwrap();
    assert!(ledger.main_seconds > 0);
    assert!(ledger.budget.active_since.is_none());
}

#[test]
fn durable_pause_during_another_probe_is_rechecked_before_claim() {
    for state in [
        json!({"status":WorkflowState::Running,"pause":{}}),
        json!({"status":WorkflowState::Cancelled}),
        json!({"status":"future-state"}),
    ] {
        let (_temp, mut c) = fixture();
        probe(&mut c, "ready", "exit 0");
        let change = format!(
            "printf '%s' '{}' > '{}'",
            json!({"a-ready-attempt-1":state}),
            c.workspace.join("states.json").display()
        );
        probe(&mut c, "change", &change);
        add(&c, "a-ready", "ready", false);
        add(&c, "b-change", "change", false);
        run_selected(&c, &["a-ready", "b-change"], 20);
        assert_eq!(
            events(&c),
            ["probe:ready", "probe:change", "acquire:b-change-attempt-1"]
        );
        let job: Job = storage::read(&job_path(&c, "a-ready")).unwrap();
        assert!(job.last_error.is_none() && job.launch.is_none());
    }
}

#[test]
fn capacity_launch_failure_keeps_unscanned_same_track_candidates() {
    let (_temp, mut c) = fixture();
    c.max_agents = 1;
    probe(&mut c, "ready", "exit 0");
    add(&c, "a-first", "ready", false);
    add(&c, "b-next", "ready", false);
    run_selected(&c, &["a-first", "b-next"], 20);
    assert_eq!(
        events(&c),
        [
            "probe:ready",
            "acquire:a-first-attempt-1",
            "probe:ready",
            "acquire:b-next-attempt-1"
        ]
    );
}

#[test]
#[ignore = "real CLI/workflow/bwrap/Xvfb with explicit fake Codex; set LAB_CONTROLLER_BIN, LAB_WORKFLOW_BIN, LAB_COMPUTER_USE_BIN, LAB_DESKTOP_DEPENDENCIES"]
fn actual_cli_mixed_prerequisites_finish_before_first_lease() {
    let (temp, mut c) = tests::scheduler_fixture();
    c.max_agents = 8;
    c.task_timeout_seconds = 45;
    let sandbox_first = std::env::var_os("LAB_COHORT_SANDBOX_FIRST").is_some();
    if sandbox_first {
        c.tools.get_mut("repo-sandbox").unwrap().binary =
            PathBuf::from(std::env::var("LAB_SANDBOX_BIN").unwrap());
    }
    let controller = PathBuf::from(std::env::var("LAB_CONTROLLER_BIN").unwrap());
    let desktop_source = PathBuf::from(std::env::var("LAB_DESKTOP_DEPENDENCIES").unwrap());
    let computer = PathBuf::from(std::env::var("LAB_COMPUTER_USE_BIN").unwrap());
    // Use a task-owned copy of already authenticated desktop dependencies. This
    // tests a controller-unprepared target, not network download performance.
    let tools = c.state_dir.join("tools");
    fs::create_dir_all(&tools).unwrap();
    assert!(
        std::process::Command::new("cp")
            .arg("-a")
            .arg(&desktop_source)
            .arg(tools.join("desktop"))
            .status()
            .unwrap()
            .success()
    );
    let installed = tools.join("computer-use");
    std::fs::copy(computer, &installed).unwrap();
    fs::set_permissions(&installed, fs::Permissions::from_mode(0o500)).unwrap();
    let desktop_probe = tools.join("computer-probe");
    let desktop_ready = temp.path().join("desktop-ready-nanos");
    fs::write(
        &desktop_probe,
        format!(
            "#!/bin/sh\nif [ \"$1\" = '--help' ]; then date +%s%N > '{}'; fi\nexec '{}' \"$@\"\n",
            desktop_ready.display(),
            installed.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&desktop_probe, fs::Permissions::from_mode(0o500)).unwrap();
    let binding = c.tools.get_mut("computer-use-cli").unwrap();
    binding.binary = desktop_probe;
    binding.probe = vec!["--help".into()];
    fs::write(c.superpod.join("Cargo.lock"), "version = 4\npackage = []\n").unwrap();
    git(&c.superpod, &["add", "Cargo.lock"]).unwrap();
    git(
        &c.superpod,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-m",
            "empty locked cache fixture",
        ],
    )
    .unwrap();
    let mut ids = vec![];
    for index in 0..8 {
        let id = format!("mixed-{index:02}");
        let required = match index {
            0 if sandbox_first => vec!["repo-sandbox"],
            6 => vec!["qualitygate-cli"],
            7 => vec!["computer-use-cli"],
            _ => vec![],
        };
        enqueue(
            &c,
            serde_json::from_value(json!({
                "id":id,"role":"research","repository":"superpod","prompt":"EIGHT_PARALLEL",
                "max_attempts":1,"use_memory":false,"required_tools":required
            }))
            .unwrap(),
        )
        .unwrap();
        ids.push(id);
    }
    let config = temp.path().join("config.toml");
    let mut encoded = format!(
        "schema_version = 1\nworkspace = {}\nstate_dir = {}\nsuperpod = {}\ncodex = {}\nworkflow = {}\ndaily_seconds = {}\nmax_agents = {}\ntask_timeout_seconds = {}\nrequire_latest = false\n[models]\n",
        json!(c.workspace),
        json!(c.state_dir),
        json!(c.superpod),
        json!(c.codex),
        json!(c.workflow),
        c.daily_seconds,
        c.max_agents,
        c.task_timeout_seconds
    );
    for (role, model) in &c.models {
        encoded.push_str(&format!("{} = {}\n", json!(role), json!(model)));
    }
    for (name, binding) in &c.tools {
        encoded.push_str(&format!(
            "[tools.{}]\nbinary = {}\nrepository = {}\nprobe = {}\n",
            json!(name),
            json!(binding.binary),
            json!(binding.repository),
            json!(binding.probe)
        ));
    }
    fs::write(&config, encoded).unwrap();
    assert_eq!(
        serde_json::to_value(Config::load(&config).unwrap()).unwrap(),
        serde_json::to_value(&c).unwrap()
    );
    let scope = c.state_dir.join("private/cohort.json");
    storage::write(&scope, &ids).unwrap();
    let output = process::capture(
        controller.to_str().unwrap(),
        &[
            "--config".into(),
            config.display().to_string(),
            "run".into(),
            "--max-seconds".into(),
            "110".into(),
            "--task-ids-file".into(),
            scope.display().to_string(),
        ],
        temp.path(),
        Duration::from_secs(120),
    )
    .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let w = workflow(&c);
    let mut intervals = BTreeMap::new();
    let mut leases = BTreeMap::new();
    let mut receipts = BTreeMap::new();
    for job in jobs(&c).unwrap() {
        let state = w.status(&job.run_id).unwrap();
        assert_eq!(
            WorkflowState::from_value(&state["status"]),
            Some(WorkflowState::Succeeded),
            "{job:?}: {state}"
        );
        receipts.insert(job.task.id.clone(), w.verify(&job.run_id).unwrap());
        let log = c
            .state_dir
            .join("runs")
            .join(&job.run_id)
            .join("process/stdout.jsonl");
        let times: Vec<u128> = fs::read_to_string(log)
            .unwrap()
            .lines()
            .filter_map(|s| s.parse().ok())
            .collect();
        assert_eq!(times.len(), 2);
        intervals.insert(job.task.id.clone(), [times[0], times[1]]);
        let lease: Value = storage::read(
            &w.work_dir
                .join("host-leases")
                .join(format!("{}.json", job.run_id)),
        )
        .unwrap();
        leases.insert(
            job.task.id,
            lease["issued_at_unix_ms"].as_u64().unwrap() as u128 * 1_000_000,
        );
    }
    let first_lease = *leases.values().min().unwrap();
    let desktop_prepared: u128 = fs::read_to_string(&desktop_ready)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    let cache_marker = c
        .state_dir
        .join("runs/mixed-06-attempt-1/build-cache-seed.json");
    let cache_prepared = fs::metadata(&cache_marker)
        .unwrap()
        .modified()
        .unwrap()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let mut changes = vec![];
    for times in intervals.values() {
        changes.push((times[0], 1_i32));
        changes.push((times[1], -1_i32));
    }
    changes.sort();
    let (mut live, mut peak) = (0, 0);
    for (_, change) in changes {
        live += change;
        peak = peak.max(live);
    }
    let report = json!({"kind":"explicit_fake_provider_real_cli_workflow_bwrap_desktop", "max_agents":8,"sandbox_first":sandbox_first,
        "peak":peak,"intervals_ns":intervals,"lease_issued_ns":leases,"first_lease_ns":first_lease,
        "desktop_prepared_probe_ns":desktop_prepared,"cache_prepared_ns":cache_prepared,
        "workflow_verification":receipts,"limits":"Desktop dependencies copied from authenticated existing cache; no cold network download, model quality or RSI claim."});
    if let Some(path) = std::env::var_os("LAB_COHORT_EVIDENCE_DIR") {
        let path = PathBuf::from(path);
        fs::create_dir_all(&path).unwrap();
        storage::write(&path.join("mixed-cohort.json"), &report).unwrap();
        fs::write(path.join("controller.stdout.json"), output.stdout).unwrap();
        fs::write(path.join("controller.stderr.log"), output.stderr).unwrap();
    }
    println!("{report}");
    assert!(
        cache_prepared <= first_lease,
        "cache preparation must precede the first cohort lease"
    );
    assert!(
        desktop_prepared <= first_lease,
        "desktop preparation/probe must precede the first cohort lease"
    );
    assert_eq!(
        peak, 8,
        "the bounded prepared cohort must overlap with this deterministic provider"
    );
    assert_eq!(live, 0);
}

#[test]
fn dependency_changed_by_later_probe_is_rechecked_before_claim() {
    let (_temp, mut c) = fixture();
    probe(&mut c, "ready", "exit 0");
    let change = format!(
        "printf '%s' '{}' > '{}'",
        json!({"parent-attempt-1":{"status":WorkflowState::Failed}}),
        c.workspace.join("states.json").display()
    );
    probe(&mut c, "change", &change);
    let parent = add(&c, "parent", "ready", false);
    enqueue(&c, serde_json::from_value(json!({
        "id":"a-dependent","role":"research","repository":"superpod","prompt":"dependency fixture",
        "max_attempts":1,"use_memory":false,"dependencies":["parent"],"required_tools":["ready"]
    })).unwrap()).unwrap();
    add(&c, "b-change", "change", false);
    storage::write(
        &c.workspace.join("states.json"),
        &json!({"parent-attempt-1":{"status":WorkflowState::Succeeded,"frames":{"1":{"nodes":{"task":{"outputs":{"result":serde_json::to_string(&json!({
            "receipt_protocol":2,"config_digest":parent.config_digest,"agent_report":{"summary":"fixture parent","findings":[],"sources":[],"limitations":["offline fixture"],"next_tasks":[]},
            "source_commit":parent.source_commit,"superpod_commit":parent.superpod_commit,
            "prompt_digest":parent.prompt_digest,"model":parent.model,"research_inputs":null,"post_success_protocol":1
        })).unwrap()}}}}}}}),
    )
    .unwrap();
    run_selected(&c, &["parent", "a-dependent", "b-change"], 20);
    assert_eq!(
        events(&c),
        ["probe:ready", "probe:change", "acquire:b-change-attempt-1"]
    );
    let job: Job = storage::read(&job_path(&c, "a-dependent")).unwrap();
    assert!(job.last_error.is_none() && job.launch.is_none());
}

#[test]
fn cache_preparation_failure_does_not_consume_capacity() {
    let (_temp, mut c) = fixture();
    c.max_agents = 1;
    probe(&mut c, "ready", "exit 0");
    let broken = add(&c, "a-broken-cache", "qualitygate-cli", false);
    let lock = c.workspace.join("unsafe-cache-lock");
    fs::write(&lock, "version = 4\npackage = []\n").unwrap();
    std::os::unix::fs::symlink(&lock, broken.worktree.join("Cargo.lock")).unwrap();
    add(&c, "b-ready", "ready", false);
    run_selected(&c, &["a-broken-cache", "b-ready"], 20);
    assert_eq!(events(&c), ["probe:ready", "acquire:b-ready-attempt-1"]);
    let job: Job = storage::read(&job_path(&c, "a-broken-cache")).unwrap();
    assert!(
        job.last_error
            .unwrap()
            .contains("build cache preparation failed before claim")
    );
    assert_eq!(job.attempt, 1);
    assert!(job.launch.is_none());
}

#[test]
fn due_retry_prepares_one_new_attempt_after_previous_capacity_fails() {
    let (_temp, mut c) = fixture();
    c.max_agents = 1;
    probe(&mut c, "ready", "exit 0");
    add(&c, "a-first", "ready", false);
    let mut job = enqueue(
        &c,
        serde_json::from_value(json!({
            "id":"b-retry","role":"research","repository":"superpod","prompt":"retry fixture",
            "max_attempts":2,"use_memory":false,"required_tools":["ready"]
        }))
        .unwrap(),
    )
    .unwrap();
    job.last_error = Some("previous bounded failure".into());
    job.retry_after = Some(now() - 1);
    storage::write(&job_path(&c, &job.task.id), &job).unwrap();
    run_selected(&c, &["a-first", "b-retry"], 20);
    assert_eq!(
        events(&c),
        [
            "probe:ready",
            "acquire:a-first-attempt-1",
            "probe:ready",
            "acquire:b-retry-attempt-2"
        ]
    );
    let job: Job = storage::read(&job_path(&c, "b-retry")).unwrap();
    assert_eq!(job.attempt, 2);
    assert!(job.retry_after.is_none());
}

#[test]
fn freshness_is_reverified_after_all_probes_without_claiming_stale_tasks() {
    const CHILD: &str = "LAB_COHORT_FRESHNESS_CHILD_CONFIG";
    if let Some(path) = std::env::var_os(CHILD) {
        let c: Config = storage::read(Path::new(&path)).unwrap();
        run_selected(&c, &["a-ready", "b-drift"], 30);
        return;
    }
    let (temp, mut c) = fixture();
    fs::create_dir_all(c.superpod.join("knowledge/software/ai-sdlc")).unwrap();
    fs::write(
        c.superpod.join("knowledge/software/ai-sdlc/insights.md"),
        "fixture insights\n",
    )
    .unwrap();
    git(
        &c.superpod,
        &["add", "knowledge/software/ai-sdlc/insights.md"],
    )
    .unwrap();
    git(
        &c.superpod,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-m",
            "freshness fixture",
        ],
    )
    .unwrap();
    let commit = git(&c.superpod, &["rev-parse", "HEAD"]).unwrap();
    fs::write(temp.path().join("release"), "1").unwrap();
    let workflow_script = fs::read_to_string(&c.workflow).unwrap().replace(
        "root = pathlib.Path(__file__).parent",
        &format!("root = pathlib.Path({})", json!(temp.path())),
    );
    let mut snapshots = vec![];
    for (name, tool) in &mut c.tools {
        let installed = temp.path().join("skills").join(name);
        fs::create_dir_all(&installed).unwrap();
        fs::write(installed.join("SKILL.md"), "offline fixture\n").unwrap();
        let binary = installed.join("tool");
        match name.as_str() {
            "workflow-cli" => fs::write(&binary, &workflow_script).unwrap(),
            "into-markdown" => fs::write(
                &binary,
                format!(
                    "#!/bin/sh\nprintf 'probe:drift\\n' >> '{}'/events\nprintf 2 > '{}'/release\n",
                    temp.path().display(),
                    temp.path().display()
                ),
            )
            .unwrap(),
            _ => {
                std::fs::copy("/bin/true", &binary).unwrap();
            }
        }
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
        tool.binary = binary.clone();
        snapshots.push(freshness::SkillSnapshot {
            name: name.clone(),
            repository: format!("coolplayagent/{name}"),
            installed_path: installed.clone(),
            runtime_path: binary.clone(),
            tree_sha256: freshness::skill_tree_digest(&installed).unwrap(),
            runtime_sha256: storage::digest(&fs::read(binary).unwrap()),
            release_tag: Some("v1".into()),
            release_id: Some(1),
            source_commit: None,
            source_qualitygate: None,
            checked_at: "2026-01-01T00:00:00Z".into(),
        });
    }
    c.workflow = c.tools["workflow-cli"].binary.clone();
    c.require_latest = true;
    let manifest = temp.path().join("skills.json");
    c.skills_manifest = Some(manifest.clone());
    let skills = freshness::SkillsManifest {
        schema_version: 1,
        skills: snapshots,
    };
    storage::write(&manifest, &skills).unwrap();
    let mut repositories = BTreeMap::new();
    let names = c
        .tools
        .keys()
        .cloned()
        .chain(["agent-research-lab".into(), "superpod".into()]);
    for name in names {
        let checkout = c.state_dir.join("sources").join(format!("{name}-{commit}"));
        fs::create_dir_all(checkout.parent().unwrap()).unwrap();
        git(
            &c.superpod,
            &[
                "worktree",
                "add",
                "--detach",
                checkout.to_str().unwrap(),
                &commit,
            ],
        )
        .unwrap();
        repositories.insert(
            name.clone(),
            inputs::RepositoryInput {
                upstream: freshness::RemoteSnapshot {
                    repository: if name == "superpod" {
                        "stevetdp/superpod".into()
                    } else {
                        format!("coolplayagent/{name}")
                    },
                    default_branch: "main".into(),
                    commit: commit.clone(),
                    checked_at: "2026-01-01T00:00:00Z".into(),
                },
                checkout,
            },
        );
    }
    let inputs = inputs::ExecutionInputs {
        repositories,
        skills,
        insights: "fixture insights\n".into(),
        insights_sha256: storage::digest(b"fixture insights\n"),
        shared_knowledge: None,
    };
    for (id, tool) in [("a-ready", "relay-memory"), ("b-drift", "into-markdown")] {
        enqueue_with_inputs(
            &c,
            serde_json::from_value(json!({
                "id":id,"role":"research","repository":"superpod","prompt":"freshness fixture",
                "max_attempts":1,"use_memory":false,"required_tools":[tool]
            }))
            .unwrap(),
            Some(inputs.clone()),
        )
        .unwrap();
    }
    let mock_bin = temp.path().join("bin");
    fs::create_dir(&mock_bin).unwrap();
    let gh = mock_bin.join("gh");
    fs::write(
        &gh,
        format!(
            r#"#!/usr/bin/python3
import json, pathlib, sys
root = pathlib.Path({root})
if sys.argv[1:3] == ['repo','view']:
    result = {{'nameWithOwner':sys.argv[3], 'defaultBranchRef':{{'name':'main'}}}}
elif sys.argv[1] == 'api' and sys.argv[2].endswith('/releases/latest'):
    release = int((root / 'release').read_text())
    result = {{'draft':False, 'prerelease':False, 'id':release, 'tag_name':'v'+str(release)}}
elif sys.argv[1] == 'api' and '/commits/' in sys.argv[2]:
    result = {{'sha':{commit}}}
else:
    raise RuntimeError('unexpected offline GitHub request')
print(json.dumps(result))
"#,
            root = json!(temp.path()),
            commit = json!(commit)
        ),
    )
    .unwrap();
    fs::set_permissions(&gh, fs::Permissions::from_mode(0o700)).unwrap();
    let configuration = temp.path().join("child-config.json");
    storage::write(&configuration, &c).unwrap();
    // Process-local PATH replaces only the remote boundary; no global env mutation
    // or freshness helper substitution is needed, including under parallel tests.
    let output = process::capture_env(
        std::env::current_exe().unwrap().to_str().unwrap(),
        &[
            "--exact".into(),
            "cohort_tests::freshness_is_reverified_after_all_probes_without_claiming_stale_tasks"
                .into(),
            "--nocapture".into(),
        ],
        temp.path(),
        Duration::from_secs(40),
        &[
            (CHILD.into(), configuration.display().to_string()),
            (
                "PATH".into(),
                format!("{}:{}", mock_bin.display(), std::env::var("PATH").unwrap()),
            ),
        ],
    )
    .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(events(&c), ["probe:drift"]);
    for id in ["a-ready", "b-drift"] {
        let job: Job = storage::read(&job_path(&c, id)).unwrap();
        assert!(
            job.last_error
                .unwrap()
                .contains("latest-baseline admission blocked")
        );
        assert!(job.launch.is_none());
        assert_eq!(job.attempt, 1);
    }
}
