//! Run only against the pinned, pre-split library to produce public synthetic fixtures.
use lab_old::{
    config::Config,
    multi_agent::TeamBinding,
    runtime::{Job, Launch, Task},
    storage,
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::Path};

fn capture<T: DeserializeOwned + Serialize>(
    out: &Path,
    name: &str,
    input: Value,
    digests: &mut BTreeMap<String, String>,
) -> T {
    let value: T = serde_json::from_value(input.clone()).unwrap();
    let serialized = serde_json::to_vec(&value).unwrap();
    fs::write(
        out.join(format!("{name}.input.json")),
        serde_json::to_vec_pretty(&input).unwrap(),
    )
    .unwrap();
    fs::write(out.join(format!("{name}.serialized.json")), &serialized).unwrap();
    digests.insert(name.into(), storage::digest(&serialized));
    value
}

fn main() {
    let destination = std::env::args().nth(1).expect("fixture output directory");
    let out = Path::new(&destination);
    fs::create_dir_all(out).unwrap();
    let mut digests = BTreeMap::new();
    let tool_names = [
        "workflow-cli",
        "relay-knowledge",
        "into-markdown",
        "qualitygate-cli",
        "computer-use-cli",
        "relay-memory",
        "repo-sandbox",
    ];
    let tools: BTreeMap<_, _> = tool_names
        .iter()
        .map(|name| {
            (
                *name,
                json!({"binary":format!("/synthetic/tools/{name}"),
                "repository":format!("/synthetic/workspace/{name}"),"probe":["--version"]}),
            )
        })
        .collect();
    let legacy_config_input = json!({
        "schema_version":1,"workspace":"/synthetic/workspace",
        "state_dir":"/synthetic/workspace/lab/.lab","superpod":"/synthetic/workspace/superpod",
        "codex":"codex","workflow":"/synthetic/tools/workflow-cli",
        "daily_seconds":43200,"max_agents":8,"task_timeout_seconds":1800,
        "models":{"research":"synthetic-research","implement":"synthetic-implement","review":"synthetic-review"},
        "tools":tools,"skills_manifest":"/synthetic/workspace/lab/.lab/latest/skills.json"
    });
    let legacy_config: Config = capture(
        out,
        "legacy-config",
        legacy_config_input.clone(),
        &mut digests,
    );
    legacy_config.validate().unwrap();
    let bridge = json!({"kind":"json_process","program":"/synthetic/tools/bridge",
        "args":["--fixture","中文"],"env_allowlist":["OPENAI_API_KEY"],
        "capabilities":{"structured_result":true,"read_workspace":true,"write_workspace":true,"tool_execution":true,"desktop":true}});
    let mut full_config_input = legacy_config_input;
    full_config_input["agent_backends"] = json!({"synthetic-bridge":bridge,"synthetic-codex":{"kind":"codex","program":"synthetic-codex"}});
    full_config_input["role_backends"] = json!({"implement":"synthetic-bridge","review":"synthetic-bridge","research":"synthetic-codex"});
    full_config_input["multi_agent"] = json!({"shared_research":true});
    full_config_input["require_latest"] = json!(true);
    let full_config: Config = capture(out, "full-config", full_config_input, &mut digests);
    full_config.validate().unwrap();

    let legacy_task_input = json!({"id":"legacy-task","role":"research","repository":"superpod","prompt":"Independent synthetic task.\nNo runtime invocation."});
    let legacy_task: Task = capture(out, "legacy-task", legacy_task_input.clone(), &mut digests);
    let full_team_input = json!({"id":"compat-team","cell":"cell-01","topics":["research","protocol"],"query":"证据 & counterexample"});
    let full_task_input = json!({"id":"full-task","role":"implement","repository":"agent-research-lab",
        "prompt":"Synthetic multilingual contract: 证据, quote \"x\", newline\nsecond line.",
        "prompt_version":"implement-v3","communication":full_team_input,"max_attempts":2,
        "use_memory":true,"depth":2,"write":true,"exploratory":true,
        "dependencies":["previous-task"],"required_tools":["workflow-cli","relay-memory"]});
    let full_task: Task = capture(out, "full-task", full_task_input.clone(), &mut digests);

    let legacy_launch_input = json!({"pid":1234,"process_start":"synthetic-start",
        "lease":{"run_id":"legacy-task-attempt-1","epoch":1,"issued_at_unix_ms":1700000000000u64,"expires_at_unix_ms":1700000060000u64},
        "attempt":{"request":{"synthetic":true}},"log_dir":"/synthetic/state/runs/legacy-task-attempt-1",
        "historical_unknown_metadata":"previous Job/Launch decoders intentionally ignore unknown fields"});
    let _legacy_launch: Launch = capture(
        out,
        "legacy-launch",
        legacy_launch_input.clone(),
        &mut digests,
    );
    let legacy_job_input = json!({"task":legacy_task_input,"model":"synthetic-research",
        "source_commit":"a".repeat(40),"superpod_commit":"b".repeat(40),
        "prompt_digest":storage::digest(legacy_task.prompt.as_bytes()),"config_digest":digests["legacy-config"],
        "worktree":"/synthetic/state/worktrees/legacy-task","run_id":"legacy-task-attempt-1","attempt":1,
        "last_error":null,"launch":legacy_launch_input,"historical_unknown_metadata":{"retained_behavior":"ignored"}});
    let legacy_job: Job = capture(out, "legacy-job", legacy_job_input, &mut digests);

    let mut repositories = BTreeMap::new();
    for name in tool_names
        .into_iter()
        .chain(["agent-research-lab", "superpod"])
    {
        repositories.insert(
            name,
            json!({"upstream":{"repository":format!("coolplayagent/{name}"),
            "default_branch":"main","commit":"c".repeat(40),"checked_at":"2026-10-10T00:00:00Z"},
            "checkout":format!("/synthetic/state/sources/{name}/{}", "c".repeat(40))}),
        );
    }
    let skills: Vec<_> = tool_names.iter().map(|name|json!({"name":name,
        "repository":format!("coolplayagent/{name}"),"installed_path":format!("/synthetic/skills/{name}"),
        "runtime_path":format!("/synthetic/skills/{name}/cli"),"tree_sha256":"d".repeat(64),
        "runtime_sha256":"e".repeat(64),"release_tag":"v0.0.0-synthetic","release_id":7,
        "source_commit":null,"checked_at":"2026-10-10T00:00:00Z"})).collect();
    let excerpt = "Synthetic SuperPOD excerpt, never copied from production.";
    let inputs = json!({"repositories":repositories,"skills":{"schema_version":1,"skills":skills},
        "insights":"Synthetic local evidence reference.","insights_sha256":storage::digest(b"Synthetic local evidence reference."),
        "shared_knowledge":{"superpod_commit":"b".repeat(40),"documents":[{"path":"knowledge/index.md",
            "source_sha256":storage::digest(excerpt.as_bytes()),"source_bytes":excerpt.len(),"excerpt":excerpt,
            "excerpt_sha256":storage::digest(excerpt.as_bytes()),"truncated":false}]}});
    let full_binding = json!({"id":"synthetic-bridge","spec":bridge,"executable":"/synthetic/tools/bridge","executable_sha256":"f".repeat(64)});
    let mut full_job_input = json!({"task":full_task_input,"model":"synthetic-implement","backend":full_binding,
        "source_commit":"c".repeat(40),"baseline_commit":"a".repeat(40),"research_inputs":inputs,
        "superpod_commit":"b".repeat(40),"prompt_digest":storage::digest(full_task.prompt.as_bytes()),
        "config_digest":digests["full-config"],"worktree":"/synthetic/state/worktrees/full-task",
        "run_id":"full-task-attempt-2","attempt":2,"last_error":"Synthetic previous attempt diagnostic.","launch":null,"retry_after":1700000100});
    let full_job_without_launch: Job = serde_json::from_value(full_job_input.clone()).unwrap();
    let full_team: TeamBinding = serde_json::from_value(full_team_input.clone()).unwrap();
    let full_member = full_team.member(&full_job_without_launch).unwrap();
    let full_launch_input = json!({"pid":2345,"process_start":"synthetic-full-start",
        "lease":{"run_id":"full-task-attempt-2","owner":"synthetic-controller","acquisition_id":"synthetic-acquisition","epoch":2,"issued_at_unix_ms":1700000000000u64,"expires_at_unix_ms":1700000060000u64},
        "attempt":{"request":{"synthetic":true},"grant":{"request_digest":format!("sha256:{}","9".repeat(64))}},
        "log_dir":"/synthetic/state/runs/full-task-attempt-2","tools":{"workflow-cli":{"runtime_sha256":"e".repeat(64)}},
        "git_dir":"/synthetic/repos/.git/worktrees/full-task","backend_request_sha256":"8".repeat(64),
        "communication":{"member":full_member,"context":{"schema_version":1,"cohort_id":full_member.cohort_id,
            "revision":3,"as_of":1700000000u64,"message_ids":["7".repeat(64)],"truncated":true,
            "text":"Untrusted synthetic proposal.\nNot research evidence.","digest":storage::digest(b"synthetic context binding")}},
        "communication_member":full_member,"communication_gap":"Synthetic optional-channel diagnostic."});
    let _full_launch: Launch = capture(out, "full-launch", full_launch_input.clone(), &mut digests);
    full_job_input["launch"] = full_launch_input;
    let full_job: Job = capture(out, "full-job", full_job_input, &mut digests);

    let legacy_team_input = json!({"id":"compat-legacy-team"});
    let legacy_team: TeamBinding = serde_json::from_value(legacy_team_input.clone()).unwrap();
    let legacy_member = legacy_team.member(&legacy_job).unwrap();
    assert_eq!(full_member, full_team.member(&full_job).unwrap());
    for (name, team, member) in [
        ("legacy", legacy_team_input, legacy_member),
        ("full", full_team_input, full_member),
    ] {
        fs::write(
            out.join(format!("{name}-team.input.json")),
            serde_json::to_vec_pretty(&team).unwrap(),
        )
        .unwrap();
        let encoded = serde_json::to_vec(&member).unwrap();
        fs::write(out.join(format!("{name}-member.serialized.json")), &encoded).unwrap();
        digests.insert(format!("{name}-member"), storage::digest(&encoded));
        digests.insert(format!("{name}-cohort-id"), member.cohort_id);
        digests.insert(format!("{name}-authority"), member.authority_sha256);
    }
    fs::write(
        out.join("digests.json"),
        serde_json::to_vec_pretty(&digests).unwrap(),
    )
    .unwrap();
    println!(
        "captured {} independent digest/identity expectations from the pre-split library",
        digests.len()
    );
}
