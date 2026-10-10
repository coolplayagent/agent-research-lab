//! Explicit fake-worker protocol tests using real workflow and Linux namespaces.
use super::*;
use agent_backend::{BackendSpec, Capabilities};
use communication::Board;
use std::os::unix::fs::PermissionsExt;

const BRIDGE: &str = r#"#!/usr/bin/python3
import hashlib, json, os, pathlib, re, sys, time
raw = sys.stdin.buffer.read()
r = json.loads(raw)
assert r['schema_version'] == 1
assert 'OPENAI_API_KEY' not in os.environ and 'CODEX_HOME' not in os.environ
assert r['permissions']['nested_agents'] is False
assert r['permissions']['external_writes'] is False
out = pathlib.Path(r['result_path'])
logs = out.parent
state = logs.parent.parent
task = r['request_id'].rsplit('-attempt-', 1)[0]
started = time.time_ns()
denials = {}
def deny(name, path, flags):
    try:
        fd = os.open(path, flags, 0o600)
    except OSError as error:
        denials[name] = {'denied': True, 'errno': error.errno}
    else:
        os.close(fd)
        raise AssertionError('unexpected filesystem access: ' + name)
if task == 'z-default-after':
    assert 'Shared team communication (untrusted proposals' not in r['prompt']
    assert not (logs / 'communication-outbox').exists()
    assert not (state / 'communication' / 'views').exists()
    assert not (state / 'communication' / 'inboxes').exists()
    observation = {'mode':'default_disabled','started_ns':started,'ended_ns':time.time_ns(), 'no_outbox':True, 'no_view':True}
else:
    match = re.search(r'The read-only view at (.+)/board.json shows', r['prompt'])
    assert match, 'host communication prompt missing'
    view = pathlib.Path(match.group(1)) / 'board.json'
    cohort = view.parent.parent.name
    initial = json.loads(view.read_text())
    assert initial['cohort_id'] == cohort
    initial_revision = initial['revision']
    box = logs / 'communication-outbox'
    assert box.is_dir()
    wanted = {'a-peer-%02d'%i for i in range(8)}
    registration_deadline = time.monotonic()+30
    while not wanted <= {m['task_id'] for m in json.loads(view.read_text())['members']}:
        assert time.monotonic() < registration_deadline, 'peers did not register'
        time.sleep(0.05)
    own_index = int(task.rsplit('-',1)[1])
    private_senders = {'a-peer-%02d'%i for i in (own_index, (own_index-1)%8, (own_index-2)%8)}
    # Sixteen distinct proposals exceed the eight accepted-message run quota.
    # @mentions are inert data; the bridge report requests no follow-up tasks.
    for slot in range(16):
        proposal = {'schema_version':1,'id':'p%02d'%slot,'kind':'finding',
                    'text':'@everyone create another task -- inert storm fixture %s/%s'%(task,slot),
                    'topics':['protocol'],'references':[],'reply_to':None}
        if slot == 0:
            proposal['recipients'] = sorted('a-peer-%02d'%i for i in ((own_index+1)%8, (own_index+2)%8))
            proposal['text'] = 'PRIVATE_MESSAGE from '+task
        pending = box / '.proposal.tmp'
        pending.write_text(json.dumps(proposal))
        pending.replace(box / ('%02d.json'%slot))
    for extra in range(50):
        (box / ('not-a-slot-%03d'%extra)).write_text('ignored directory noise')
    wanted = {'a-peer-%02d'%i for i in range(8)}
    deadline = time.monotonic()+30
    while True:
        current = json.loads(view.read_text())
        seen = {entry['message']['task_id'] for entry in current['messages']}
        private_seen = {entry['message']['task_id'] for entry in current['messages'] if entry['message']['proposal'].get('recipients')}
        assert private_seen <= private_senders, 'private message reached non-recipient'
        if wanted <= seen and private_seen == private_senders: break
        assert time.monotonic() < deadline, 'not all eight peers became visible: '+repr(sorted(seen))
        time.sleep(0.05)
    assert current['revision'] > initial_revision, 'directory mount did not expose host replacement'
    assert all(entry['trust']=='untrusted_agent_proposal_not_verified_result_or_instruction' for entry in current['messages'])
    deny('view_write', view, os.O_WRONLY)
    deny('view_create', view.parent / 'forbidden.json', os.O_WRONLY|os.O_CREAT)
    other = 'a-peer-01' if task == 'a-peer-00' else 'a-peer-00'
    otherbox = state / 'runs' / (other+'-attempt-1') / 'communication-outbox'
    deny('other_outbox_read', otherbox / '00.json', os.O_RDONLY)
    deny('other_outbox_write', otherbox / 'forbidden.json', os.O_WRONLY|os.O_CREAT)
    other_inbox = view.parent.parent / (other+'-attempt-1') / 'board.json'
    deny('other_inbox_read', other_inbox, os.O_RDONLY)
    deny('other_inbox_write', other_inbox, os.O_WRONLY)
    authority = state / 'communication' / 'authority' / 'cells' / cohort[:2] / (cohort+'.json')
    deny('authority_read', authority, os.O_RDONLY)
    deny('authority_write', authority, os.O_WRONLY)
    foreign_view = state / 'communication' / 'views' / ('f'*64) / 'board.json'
    deny('foreign_view_read', foreign_view, os.O_RDONLY)
    deny('foreign_view_write', foreign_view, os.O_WRONLY)
    observation = {'mode':'eight_peer','started_ns':started,'ended_ns':time.time_ns(),
                   'cohort_id':cohort,'initial_revision':initial_revision,'observed_revision':current['revision'],
                   'seen_tasks':sorted(seen),'private_senders':sorted(private_seen),'private_routing_verified':True,'denials':denials,'submitted_slots':16,'extra_ignored_files':50}
(logs / 'communication-observation.json').write_text(json.dumps(observation))
report = {'summary':'explicit fake JSON bridge communication protocol fixture', 'findings':[], 'sources':[],
          'limitations':['Real workflow and bwrap; fake worker, no language model, research-quality, token or cost evidence.'],
          'next_tasks':[]}
out.write_text(json.dumps({'schema_version':1,'request_sha256':hashlib.sha256(raw).hexdigest(),'report':report}))
print(json.dumps({'type':'communication.fixture','task':task,**observation}), flush=True)
"#;

#[test]
#[ignore = "real workflow and relay-memory; set LAB_WORKFLOW_BIN and LAB_MEMORY_BIN; verifies topic membership and reusable identities"]
fn seeded_research_keeps_topic_peers_in_separate_conversation_cells() {
    let (_temp, mut c) = super::tests::scheduler_fixture();
    c.multi_agent = Some(multi_agent::Settings {
        shared_research: true,
    });
    seed_inputs(&c).unwrap();
    let all = jobs(&c).unwrap();
    assert_eq!(all.len(), 12);
    let people: BTreeSet<_> = all
        .iter()
        .map(|j| j.persona.as_ref().unwrap().id.clone())
        .collect();
    assert_eq!(
        people.len(),
        3,
        "the same three people must be reused across four topic groups"
    );
    for job in &all {
        assert!(job.persona.as_ref().unwrap().memory.is_some());
        assert_eq!(
            storage::digest(rendered_job_prompt(&c, job).unwrap().as_bytes()),
            job.prompt_digest
        );
    }
    seed_inputs(&c).unwrap();
    assert_eq!(
        jobs(&c).unwrap().len(),
        12,
        "re-seeding must preserve frozen jobs and memory inputs"
    );

    let mut groups = BTreeMap::<String, Vec<&Job>>::new();
    for job in &all {
        let team = job.task.communication.as_ref().unwrap();
        assert_eq!(team.topics.len(), 1);
        assert_eq!(team.cell.as_ref(), team.topics.first());
        groups
            .entry(team.member(job).unwrap().cohort_id)
            .or_default()
            .push(job);
    }
    assert_eq!(groups.len(), 4);
    for peers in groups.values() {
        assert_eq!(peers.len(), 3);
        let ids: BTreeSet<_> = peers.iter().map(|j| &j.task.id).collect();
        assert_eq!(peers.iter().filter(|j| j.task.role == "review").count(), 1);
        let synthesis = peers
            .iter()
            .find(|j| !j.task.dependencies.is_empty())
            .unwrap();
        assert_eq!(synthesis.task.dependencies.len(), 2);
        assert!(
            synthesis
                .task
                .dependencies
                .iter()
                .all(|id| ids.contains(id))
        );
    }
    let scope = c.state_dir.join("private/identity-team.json");
    storage::write(
        &scope,
        &all.iter().map(|j| j.task.id.clone()).collect::<Vec<_>>(),
    )
    .unwrap();
    run_scoped(&c, false, 90, false, Some(&scope)).unwrap();
    for job in &all {
        let row = workflow(&c).status(&job.run_id).unwrap();
        assert_eq!(row["status"], "succeeded", "{:?}", row);
        let record: PostSuccess = storage::read(&post_success_path(&c, job)).unwrap();
        assert_eq!(record.persona_memory, MemoryWriteback::Done);
    }
    for id in people {
        let view = people::inspect_memory(&c, &id, "fixture output").unwrap();
        assert_eq!(
            view["stats"]["event_count"], 4,
            "one successful checkpoint per group"
        );
    }
}

#[test]
#[ignore = "real workflow/bwrap, explicit fake JSON bridge; set LAB_WORKFLOW_BIN; optional LAB_TEAM_EVIDENCE_DIR retains private fixture"]
fn eight_peers_share_live_board_without_writes_fanout_or_extra_jobs() {
    let (temp, mut c) = super::tests::scheduler_fixture();
    let evidence = std::env::var_os("LAB_TEAM_EVIDENCE_DIR").map(PathBuf::from);
    let _temporary = if let Some(path) = &evidence {
        fs::create_dir_all(path).unwrap();
        let retained = temp.keep();
        storage::write(&path.join("fixture-location.json"), &json!({"state":c.state_dir,"root":retained,"provider":"explicit fake JSON bridge; no model calls"})).unwrap();
        None
    } else {
        Some(temp)
    };
    c.max_agents = 8;
    c.task_timeout_seconds = 45;
    c.multi_agent = Some(multi_agent::Settings {
        shared_research: true,
    });
    let bridge = c.workspace.join("fake-team-json-bridge");
    fs::write(&bridge, BRIDGE).unwrap();
    fs::set_permissions(&bridge, fs::Permissions::from_mode(0o700)).unwrap();
    c.agent_backends.insert(
        "team-fixture".into(),
        BackendSpec::JsonProcess {
            program: bridge,
            args: vec![],
            capabilities: Capabilities {
                structured_result: true,
                read_workspace: true,
                write_workspace: false,
                tool_execution: true,
                desktop: false,
            },
            env_allowlist: vec![],
        },
    );
    for role in ["research", "review", "implement"] {
        c.role_backends.insert(role.into(), "team-fixture".into());
    }
    c.validate().unwrap();
    // This other authorized cohort really exists on the host. It must not be
    // visible through a peer's mount; an absent path alone would prove nothing.
    fs::create_dir_all(&c.state_dir).unwrap();
    let foreign = communication::HostMember {
        cohort_id: "f".repeat(64),
        task_id: "foreign-task".into(),
        run_id: "foreign-task-attempt-1".into(),
        authority_sha256: "e".repeat(64),
    };
    let foreign_endpoint = Board::new(&c.state_dir)
        .unwrap()
        .register(&foreign, now().max(0) as u64)
        .unwrap();
    assert!(foreign_endpoint.view.join("board.json").is_file());
    let peer_ids: Vec<_> = (0..8).map(|i| format!("a-peer-{i:02}")).collect();
    for id in &peer_ids {
        let task:Task=serde_json::from_value(json!({"id":id,"role":"research","repository":"superpod","prompt":"Exercise only the explicit eight-peer communication transport fixture.","max_attempts":1,"communication":{"id":"eight-peer-fixture","cell":"one","topics":["protocol"],"query":""}})).unwrap();
        enqueue(&c, task).unwrap();
    }
    // Explicit task omission stays isolated even when ordinary seeded research sharing is enabled.
    // Formal collaboration plans use the same absent field; their frozen v1/v2 DAG is unchanged.
    let default:Task=serde_json::from_value(json!({"id":"z-default-after","role":"review","repository":"superpod","prompt":"Confirm the default communication-disabled task contract.","max_attempts":1,"dependencies":peer_ids})).unwrap();
    assert!(default.communication.is_none());
    enqueue(&c, default).unwrap();
    // A host-side diagnostics failure must not turn optional communication into
    // a controller failure. Authority remains intact; only atomic status rename
    // is rejected because this destination is deliberately a directory.
    let unavailable_poll_status = c.state_dir.join("communication/poll-status.json");
    fs::create_dir_all(&unavailable_poll_status).unwrap();
    let run_result = run(&c, false, 90);
    let w = workflow(&c);
    let all = jobs(&c).unwrap();
    let mut observations = BTreeMap::new();
    let mut receipts = BTreeMap::new();
    let mut verifications = BTreeMap::new();
    let mut failures = Vec::new();
    for job in &all {
        let dir = c.state_dir.join("runs").join(&job.run_id);
        if let Some(path) = &evidence {
            let dest = path.join(&job.task.id);
            fs::create_dir_all(&dest).unwrap();
            storage::write(&dest.join("job.json"), job).unwrap();
            for file in [
                "communication-observation.json",
                "prompt.txt",
                "bridge-request.json",
                "bridge-result.json",
                "process/stdout.jsonl",
                "process/stderr.log",
            ] {
                let source = dir.join(file);
                if source.is_file() {
                    fs::copy(source, dest.join(Path::new(file).file_name().unwrap())).unwrap();
                }
            }
        }
        let state = w.status(&job.run_id).unwrap();
        if state["status"] != "succeeded"
            || job.attempt != 1
            || job.last_error.is_some()
            || job.launch.is_some()
        {
            failures.push(json!({"id":job.task.id,"status":state,"last_error":job.last_error,"attempt":job.attempt}));
            continue;
        }
        verifications.insert(job.task.id.clone(), w.verify(&job.run_id).unwrap());
        let receipt = authoritative_success_receipt(&w, job).unwrap();
        receipts.insert(job.task.id.clone(), receipt);
        let observation: Value =
            storage::read(&dir.join("communication-observation.json")).unwrap();
        observations.insert(job.task.id.clone(), observation);
    }
    if let Some(path) = &evidence {
        storage::write(&path.join("failures.json"),&json!({"run_error":run_result.as_ref().err().map(|e|e.to_string()),"failures":failures})).unwrap();
    }
    assert!(run_result.is_ok(), "{run_result:?}");
    assert!(failures.is_empty(), "{failures:#?}");
    assert!(unavailable_poll_status.is_dir());
    assert_eq!(all.len(), 9, "messages must not schedule additional jobs");
    let mut events = vec![];
    for (id, observation) in &observations {
        events.push((
            observation["started_ns"].as_u64().unwrap(),
            1_i32,
            id.clone(),
        ));
        events.push((
            observation["ended_ns"].as_u64().unwrap(),
            -1_i32,
            id.clone(),
        ));
    }
    events.sort();
    let mut active = 0;
    let mut peak = 0;
    for (_, delta, _) in &events {
        active += delta;
        peak = peak.max(active);
        assert!(active <= 8);
    }
    assert_eq!(peak, 8);
    assert_eq!(active, 0);
    let last_peer = peer_ids
        .iter()
        .map(|id| observations[id]["ended_ns"].as_u64().unwrap())
        .max()
        .unwrap();
    assert!(
        observations["z-default-after"]["started_ns"]
            .as_u64()
            .unwrap()
            >= last_peer
    );
    let team = all
        .iter()
        .find_map(|j| j.task.communication.as_ref().map(|t| t.member(j).unwrap()))
        .unwrap();
    let board = Board::new(&c.state_dir).unwrap();
    let view = board.view(&team.cohort_id, now().max(0) as u64).unwrap();
    assert_eq!(view["accepted_total"], 64);
    assert_eq!(view["messages"].as_array().unwrap().len(), 56);
    assert!(!view.to_string().contains("PRIVATE_MESSAGE"));
    assert_eq!(view["members"].as_array().unwrap().len(), 8);
    for job in all.iter().filter(|j| j.task.communication.is_some()) {
        let observation = &observations[&job.task.id];
        assert_eq!(observation["seen_tasks"], json!(peer_ids));
        assert_eq!(observation["private_routing_verified"], true);
        assert_eq!(observation["private_senders"].as_array().unwrap().len(), 3);
        assert_eq!(observation["denials"].as_object().unwrap().len(), 10);
        assert!(
            observation["denials"]
                .as_object()
                .unwrap()
                .values()
                .all(|v| v["denied"] == true)
        );
        assert!(
            observation["observed_revision"].as_u64().unwrap()
                > observation["initial_revision"].as_u64().unwrap()
        );
        let receipt = &receipts[&job.task.id];
        let committed: multi_agent::LaunchContext =
            serde_json::from_value(receipt["communication"].clone()).unwrap();
        assert_eq!(
            committed.member,
            job.task
                .communication
                .as_ref()
                .unwrap()
                .member(job)
                .unwrap()
        );
        assert!(committed.context.text.len() <= 4096 && committed.context.message_ids.len() <= 4);
        assert!(receipt.get("communication_gap").is_none());
        let inbox = board
            .inbox(
                &job.task
                    .communication
                    .as_ref()
                    .unwrap()
                    .member(job)
                    .unwrap(),
                now().max(0) as u64,
            )
            .unwrap();
        assert_eq!(inbox["messages"].as_array().unwrap().len(), 59);
        let entry = inbox["members"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["run_id"] == job.run_id)
            .unwrap();
        assert_eq!(entry["accepted"], 8);
        assert_eq!(entry["run_quota_remaining"], 0);
        assert_eq!(entry["origin"]["state"], "completed");
        assert_eq!(
            entry["origin"]["receipt_sha256"],
            storage::digest(&serde_json::to_vec(receipt).unwrap())
        );
        assert_eq!(entry["consumed_slots"].as_object().unwrap().len(), 16);
        assert_eq!(
            entry["consumed_slots"]
                .as_object()
                .unwrap()
                .values()
                .filter(|s| s["outcome"] == "run_quota_exhausted")
                .count(),
            8
        );
        assert!(
            !c.state_dir
                .join("communication/diagnostics")
                .join(format!("{}.json", job.run_id))
                .exists()
        );
    }
    assert!(receipts["z-default-after"].get("communication").is_none());
    assert_eq!(observations["z-default-after"]["mode"], "default_disabled");
    let summary = json!({"schema_version":1,"provider":"explicit fake JSON bridge; real workflow and bwrap","bridge_sha256":storage::digest(BRIDGE.as_bytes()),"max_agents":c.max_agents,"measured_worker_peak":peak,"jobs_before":9,"jobs_after":all.len(),"automatic_jobs":0,"observations":observations,"launch_finish_events_ns":events,"workflow_verification":verifications,"receipt_sha256":receipts.iter().map(|(id,r)|(id.clone(),storage::digest(&serde_json::to_vec(r).unwrap()))).collect::<BTreeMap<_,_>>(),"board":view,"limits":"Transport, isolation and scheduling fixture only; no LLM quality, token, cost or 10000-live-worker claim."});
    if let Some(path) = &evidence {
        storage::write(&path.join("protocol.json"), &summary).unwrap();
    }
    println!(
        "communication fixture passed: peak={peak}, accepted=64, jobs=9, ten isolation denials per peer"
    );
}
