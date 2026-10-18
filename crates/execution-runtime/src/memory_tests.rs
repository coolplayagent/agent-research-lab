use super::*;
use crate::tests::completion_fixture;
use std::collections::BTreeMap;
use task::Task;

#[test]
fn memory_checkpoint_bounds_unicode_and_escaping_without_copying_provenance() {
    let (_temp, _c, job, _w) = completion_fixture(false, json!([]));
    let receipt = json!({
        "agent_report": {
            "summary":"Verify\"latest\"source\\path\n\u{0001}".repeat(1000),
            "findings":vec!["Independent counterexamples needed.".repeat(200); 8],
            "limitations":vec!["External evaluation remains incomplete.".repeat(200); 7],
            "sources":["source-payload-must-stay-out".repeat(1000)],
            "next_tasks":[]
        },
        "research_inputs":{"repositories":{},"skills":{},"insights":"provenance-must-stay-out".repeat(30000)},
        "tools":{"payload":"tool-payload-must-stay-out".repeat(1000)}
    });
    let encoded = memory_checkpoint(&job, &receipt).unwrap();
    assert!(encoded.len() <= 4096);
    let checkpoint: Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(
        checkpoint["receipt_sha256"],
        storage::digest(&serde_json::to_vec(&receipt).unwrap())
    );
    assert_eq!(checkpoint["source_commit"], job.source_commit);
    assert_eq!(checkpoint["prompt_digest"], job.prompt_digest);
    assert_eq!(checkpoint["run_id"], job.run_id);
    assert_eq!(checkpoint["truncated"], true);
    assert!(checkpoint["summary"].as_str().unwrap().ends_with('…'));
    assert_eq!(checkpoint["findings"].as_array().unwrap().len(), 3);
    assert_eq!(checkpoint["limitations"].as_array().unwrap().len(), 2);
    for field in ["research_inputs", "tools", "sources", "next_tasks"] {
        assert!(checkpoint.get(field).is_none());
    }
    for marker in [
        "provenance-must-stay-out",
        "source-payload-must-stay-out",
        "tool-payload-must-stay-out",
    ] {
        assert!(!encoded.contains(marker));
    }
    let concise = json!({"agent_report":{"summary":"Preserve the entire conclusion.","findings":["Original evidence available."],"sources":[],"limitations":["Independent review required."]}});
    let concise: Value = serde_json::from_str(&memory_checkpoint(&job, &concise).unwrap()).unwrap();
    assert_eq!(concise["summary"], "Preserve the entire conclusion.");
    assert_eq!(concise["truncated"], false);
    let prompt = remember_prompt(&"Continue research context.".repeat(500));
    assert!(prompt.chars().count() <= 512);
    assert!(prompt.ends_with('…'));
    assert_eq!(remember_prompt("Continue research"), "Continue research");
}

#[test]
#[ignore = "set LAB_MEMORY_BIN for actual bounded Chinese checkpoint writeback and retrieval"]
fn real_memory_concise_chinese_checkpoint_excludes_provenance() {
    let binary = std::env::var("LAB_MEMORY_BIN").expect("LAB_MEMORY_BIN");
    let (temp, mut c, mut job, _w) = completion_fixture(true, json!([]));
    c.tools.get_mut("relay-memory").unwrap().binary = binary.into();
    job.task.prompt =
        "Continue I005 long-running research, verify source and retain open questions.".repeat(30);
    let receipt = json!({
        "agent_report":{
            "summary":"Research checkpoint: latest source verified; next test I005 counterexamples.",
            "findings":["Research conclusions must cite frozen source and original evidence."],
            "sources":["full-source-detail-not-copied"],
            "limitations":["Independent evaluation incomplete; no capability improvement established."],
            "next_tasks":[]
        },
        "research_inputs":{"insights":"large-provenance-not-copied".repeat(30000)},
        "tools":{"runtime":"large-tool-detail-not-copied".repeat(1000)}
    });
    let checkpoint = memory_checkpoint(&job, &receipt).unwrap();
    assert!(checkpoint.len() <= 4096);
    assert!(!checkpoint.contains("large-provenance-not-copied"));
    let started = std::time::Instant::now();
    let remembered = memory_call(
        &c,
        &job,
        &temp.path().join("writeback"),
        "remember",
        Some(&checkpoint),
    )
    .unwrap();
    let elapsed = started.elapsed();
    assert!(elapsed < Duration::from_secs(20));
    assert_eq!(remembered["response"], checkpoint);
    assert!(remembered["prompt"].as_str().unwrap().chars().count() <= 512);
    assert!(remembered["prompt"].as_str().unwrap().ends_with('…'));
    job.task.prompt =
        "Continue I005 research, restore the latest source checkpoint and open counterexamples."
            .into();
    let prepared = memory_call(&c, &job, &temp.path().join("retrieval"), "prepare", None).unwrap();
    // The CLI intentionally returns only bounded response excerpts. Match
    // the exact committed event and its retained Chinese evidence text.
    assert!(
        prepared["recent_events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event["id"] == remembered["id"]
                && event["response"].as_str().is_some_and(|text| text.contains(
                    "Research conclusions must cite frozen source and original evidence."
                )))
    );
    eprintln!(
        "real concise Chinese memory writeback completed in {elapsed:?}; {} bytes",
        checkpoint.len()
    );
}

#[test]
#[ignore = "set LAB_MEMORY_BIN for real per-experiment memory continuity"]
fn real_memory_survives_attempt_change_and_isolates_experiments() {
    let binary = std::env::var("LAB_MEMORY_BIN").expect("LAB_MEMORY_BIN");
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let c = Config {
        require_latest: false,
        skills_manifest: None,
        schema_version: 1,
        workspace: root.into(),
        state_dir: root.join("state"),
        superpod: root.join("superpod"),
        codex: "unused".into(),
        multi_agent: None,
        agent_backends: BTreeMap::new(),
        role_backends: BTreeMap::new(),
        backend_models: BTreeMap::new(),
        workflow: root.join("unused"),
        daily_seconds: 3600,
        max_agents: 1,
        task_timeout_seconds: 60,
        models: Default::default(),
        tools: std::collections::BTreeMap::from([(
            "relay-memory".into(),
            config::Tool {
                binary: binary.into(),
                repository: root.into(),
                probe: vec![],
            },
        )]),
    };
    let mut job = Job {
        persona: None,
        backend: None,
        task: Task {
            persona_id: None,
            id: "memory-baseline".into(),
            role: "research".into(),
            repository: "superpod".into(),
            prompt: "Resume the bounded repository research checkpoint".into(),
            prompt_version: None,
            communication: None,
            max_attempts: None,
            use_memory: true,
            depth: 0,
            write: false,
            exploratory: false,
            dependencies: vec![],
            required_tools: vec![],
        },
        model: "fixture".into(),
        source_commit: "fixture".into(),
        baseline_commit: None,
        research_inputs: None,
        superpod_commit: "fixture".into(),
        prompt_digest: "fixture".into(),
        config_digest: "fixture".into(),
        worktree: root.into(),
        run_id: "memory-baseline-attempt-1".into(),
        attempt: 1,
        last_error: None,
        launch: None,
        retry_after: None,
    };
    let first = root.join("logs/attempt-1");
    let prepared = memory_call(&c, &job, &first, "prepare", None).unwrap();
    assert!(prepared["recent_events"].as_array().unwrap().is_empty());
    memory_call(
        &c,
        &job,
        &first,
        "remember",
        Some("Checkpoint: source inspection completed; continue independent evaluation."),
    )
    .unwrap();
    job.attempt = 2;
    job.run_id = "memory-baseline-attempt-2".into();
    let resumed = memory_call(&c, &job, &root.join("logs/attempt-2"), "prepare", None).unwrap();
    assert!(!resumed["recent_events"].as_array().unwrap().is_empty());
    job.task.id = "memory-candidate".into();
    job.run_id = "memory-candidate-attempt-1".into();
    job.attempt = 1;
    let separate = memory_call(&c, &job, &root.join("logs/candidate"), "prepare", None).unwrap();
    assert!(separate["recent_events"].as_array().unwrap().is_empty());
    for id in ["memory-baseline", "memory-candidate"] {
        assert!(
            c.state_dir
                .join("task-memory")
                .join(id)
                .join("memory.sqlite")
                .exists()
        );
    }
    assert!(!first.join("memory/memory.sqlite").exists());
}
