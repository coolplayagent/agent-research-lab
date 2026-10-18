use super::*;
use crate::{
    identity::{digest, input_digest},
    planning::{build_tasks, plan_digest, specialist_prompt},
    protocol::PROTOCOL,
};
use evolution::{PromptDefinition, PromptRegistry, PromptVersion};
use inputs::ExecutionInputs;
use serde_json::json;
use std::fs;
use task::Job;

pub(super) fn fixture() -> (tempfile::TempDir, Config, Plan, Citation) {
    fixture_protocol(PROTOCOL)
}

pub(super) fn fixture_protocol(protocol: &str) -> (tempfile::TempDir, Config, Plan, Citation) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let state = root.join("state");
    let repo = root.join("superpod");
    fs::create_dir_all(repo.join("knowledge")).unwrap();
    fs::create_dir_all(&state).unwrap();
    let content = "# Shared knowledge\nRecovery needs observable effects.\n";
    fs::write(repo.join("knowledge/index.md"), content).unwrap();
    for args in [
        vec!["init", "-q"],
        vec!["add", "knowledge/index.md"],
        vec![
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            "fixture",
        ],
    ] {
        assert!(
            std::process::Command::new("git")
                .args(args)
                .current_dir(&repo)
                .status()
                .unwrap()
                .success()
        );
    }
    let commit = String::from_utf8(
        std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&repo)
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim()
    .to_owned();
    let c:Config=serde_json::from_value(json!({"schema_version":1,"workspace":root,"state_dir":state,"superpod":repo,"codex":"fixture","workflow":"/fixture/workflow","daily_seconds":7200,"max_agents":3,"task_timeout_seconds":300,"models":{"research":"model-a","review":"model-b","implement":"model-c"},"tools":{},"require_latest":false,"skills_manifest":null})).unwrap();
    let r = PlanRequest {
        schema_version: 1,
        experiment_id: "pilot-one".into(),
        task_family: "recovery".into(),
        trial_id: "instance-one".into(),
        question: "How should interrupted effects be reconciled?".into(),
        primary_hypothesis: "Peer counterexamples improve justified revisions".into(),
        policy_version: "human-rubric-v1".into(),
        max_claims: 3,
        task_timeout_seconds: 300,
        first_arm: Arm::Baseline,
        knowledge_sources: vec!["knowledge/index.md".into()],
        slots: (0..8)
            .map(|i| Slot {
                id: format!("slot-{i}"),
                role: if i % 2 == 0 { "research" } else { "review" }.into(),
                model: if i % 2 == 0 { "model-a" } else { "model-b" }.into(),
                perspective: format!("Perspective {i}"),
                parent_prompt_version: None,
            })
            .collect(),
    };
    let prompts: Vec<_> = r
        .slots
        .iter()
        .map(|slot| {
            PromptVersion::new(PromptDefinition {
                parent_versions: vec![],
                role: slot.role.clone(),
                task_kind: protocol.into(),
                content: specialist_prompt(slot),
                change_reason: "fixture".into(),
                failure_conditions: vec!["unsupported evidence".into()],
            })
            .unwrap()
        })
        .collect();
    let mut registry = PromptRegistry::default();
    for p in &prompts {
        registry.register(p.clone()).unwrap();
    }
    storage::write(&state.join("evolution/prompts.json"), &registry).unwrap();
    let inputs:ExecutionInputs=serde_json::from_value(json!({"repositories":{"superpod":{"upstream":{"repository":"stevetdp/superpod","default_branch":"main","commit":commit,"checked_at":"2026-10-10T00:00:00Z"},"checkout":repo}},"skills":{"schema_version":1,"skills":[]},"insights":"frozen insights","insights_sha256":storage::digest(b"frozen insights")})).unwrap();
    let tasks = build_tasks(&r, &prompts, protocol).unwrap();
    let mut plan = Plan {
        schema_version: 1,
        protocol: protocol.into(),
        request: r,
        config_digest: digest(&c).unwrap(),
        input_digest: input_digest(&inputs).unwrap(),
        inputs,
        prompts,
        tasks,
        digest: String::new(),
    };
    plan.digest = plan_digest(&plan).unwrap();
    let citation = Citation {
        repository: "superpod".into(),
        commit,
        path: "knowledge/index.md".into(),
        start_line: 2,
        end_line: 2,
        sha256: storage::digest(content.as_bytes()),
        quote: "Recovery needs observable effects.".into(),
    };
    (temp, c, plan, citation)
}

pub(super) fn card(citation: &Citation) -> ClaimCard {
    ClaimCard {
        id: "claim-one".into(),
        text: "Recovery requires observability.".into(),
        evidence: vec![citation.clone()],
        counterexample_to: vec![],
        revises: vec![],
        change: Change::New,
        rationale: "The pinned source explains the condition.".into(),
        knowledge_update: Some(KnowledgeUpdate {
            target_path: "knowledge/index.md".into(),
            proposal: "Add a bounded interrupted-effect experiment.".into(),
        }),
    }
}

pub(super) fn job_and_receipt(
    c: &Config,
    plan: &Plan,
    entry: &PlannedTask,
    card: &ClaimCard,
) -> (Job, Value, Value) {
    let commit = &plan.inputs.repositories["superpod"].upstream.commit;
    let job = Job {
        persona: None,
        backend: None,
        task: entry.task.clone(),
        model: entry.model.clone(),
        source_commit: commit.clone(),
        baseline_commit: None,
        research_inputs: Some(plan.inputs.clone()),
        superpod_commit: commit.clone(),
        prompt_digest: "frozen-rendered-prompt".into(),
        config_digest: plan.config_digest.clone(),
        worktree: c.state_dir.join("worktrees").join(&entry.task.id),
        run_id: format!("{}-attempt-1", entry.task.id),
        attempt: 1,
        last_error: None,
        launch: None,
        retry_after: None,
    };
    let receipt = json!({"agent_report":{"summary":"A bounded fixture finding","findings":[serde_json::to_string(card).unwrap()],"sources":["superpod:knowledge/index.md"],"limitations":["protocol fixture"],"next_tasks":[]},"source_commit":commit,"superpod_commit":commit,"prompt_digest":job.prompt_digest,"model":job.model});
    let state = json!({"status":"succeeded","frames":{"1":{"nodes":{"task":{"outputs":{"result":serde_json::to_string(&receipt).unwrap()}}}}}});
    (job, receipt, state)
}
