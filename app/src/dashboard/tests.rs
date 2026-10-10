use super::*;

fn job() -> Job {
    serde_json::from_value(json!({
        "task":{"id":"research-one","role":"research","repository":"superpod","prompt":"PRIVATE_PROMPT",
            "dependencies":["predecessor"]},
        "model":"test-model","source_commit":"a".repeat(40),"superpod_commit":"b".repeat(40),
        "prompt_digest":"c".repeat(64),"config_digest":"d".repeat(64),
        "worktree":"/PRIVATE_WORKTREE","run_id":"research-one-attempt-1","attempt":1,"last_error":null,
        "launch":{"pid":123,"process_start":"1","lease":{"token":"PRIVATE_LEASE"},
            "attempt":{},"log_dir":"/PRIVATE_LOG","tools":{"credential":"PRIVATE_TOOL"}}
    })).unwrap()
}

#[test]
fn projection_uses_committed_context_without_exposing_raw_worker_data() {
    let receipt = json!({"communication":{"context":{"message_ids":["message-one"],"digest":"context-digest","as_of":42,"text":"PRIVATE_CONTEXT"}},
        "agent_report":{"summary":"PUBLIC_REPORT"},"candidate":{"candidate_commit":"e".repeat(40)}});
    let row = json!({"state":"succeeded","error":null,"status":{"frames":{"1":{"nodes":{"task":{"outputs":{"result":receipt.to_string()}}}}}}});
    let view = project(&job(), &row);
    assert_eq!(view["state"], "succeeded");
    assert_eq!(view["report"]["summary"], "PUBLIC_REPORT");
    assert_eq!(view["context"]["message_ids"], json!(["message-one"]));
    assert_eq!(view["dependencies"], json!(["predecessor"]));
    assert_eq!(view["candidate_commit"], "e".repeat(40));
    assert!(!view.to_string().contains("PRIVATE_"));
}

#[test]
fn http_rejects_dns_rebinding_foreign_origins_mutation_and_duplicate_hosts() {
    let addr = "127.0.0.1:8090".parse().unwrap();
    assert_eq!(allowed_request("GET /api/snapshot HTTP/1.1\r\nHost: localhost:8090\r\nSec-Fetch-Site: same-origin\r\n\r\n",addr).unwrap(), "/api/snapshot");
    for request in [
        "GET /api/snapshot HTTP/1.1\r\nHost: attacker.example:8090\r\n\r\n",
        "GET /api/snapshot HTTP/1.1\r\nHost: 127.0.0.1:8090\r\nOrigin: https://attacker.example\r\n\r\n",
        "GET /api/snapshot HTTP/1.1\r\nHost: 127.0.0.1:8090\r\nSec-Fetch-Site: cross-site\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: 127.0.0.1:8090\r\nHost: localhost:8090\r\n\r\n",
        "POST /api/snapshot HTTP/1.1\r\nHost: 127.0.0.1:8090\r\n\r\n",
    ] {
        assert!(allowed_request(request, addr).is_err());
    }
}

#[test]
fn cross_group_identity_history_pages_older_tasks_and_all_attempts_without_prompts() {
    let root = std::env::temp_dir().join(format!(
        "lab-person-history-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(root.clone());
    fs::create_dir_all(&root).unwrap();
    let c:Config=serde_json::from_value(json!({"schema_version":1,"workspace":root,"state_dir":root.join("state"),"superpod":root.join("superpod"),"codex":"unused","workflow":"/unused","daily_seconds":600,"max_agents":2,"task_timeout_seconds":60,"require_latest":false,"models":{"research":"test-model"},"tools":{}})).unwrap();
    let directory = runtime::people::directory(&c).unwrap();
    let id = directory.defaults["research"].clone();
    let mut jobs = vec![];
    for i in 0..30 {
        let mut j = job();
        j.task.id = format!("case-{i:02}");
        j.task.persona_id = Some(id.clone());
        j.attempt = 2;
        j.run_id = format!("{}-attempt-2", j.task.id);
        j.persona = Some(task::PersonaSnapshot {
            execution: None,
            id: id.clone(),
            name: directory.people[&id].name.clone(),
            revision: 1,
            soul: "frozen soul".into(),
            memory: Some(task::PersonaMemory {
                context: "PRIVATE_MEMORY".into(),
                sha256: "digest".into(),
                pack_sha256: "pack".into(),
                executable_sha256: "binary".into(),
                captured_at: 42,
            }),
        });
        storage::write(
            &c.state_dir.join("jobs").join(format!("{}.json", j.task.id)),
            &j,
        )
        .unwrap();
        jobs.push(j);
    }
    runtime::people::register_history(&c, &jobs).unwrap();
    let shared = Arc::new(RwLock::new(json!({"jobs":[]})));
    let mut after = String::new();
    let mut runs = BTreeSet::new();
    loop {
        let page = personas::history(&c, &id, &after, &shared).unwrap();
        assert!(!page.to_string().contains("PRIVATE_"));
        let rows = page["sessions"].as_array().unwrap();
        assert!(rows.len() <= 24);
        for row in rows {
            assert_eq!(row["profile"]["person_id"], id);
            assert!(runs.insert(row["run_id"].as_str().unwrap().to_owned()));
        }
        match page["next_after"].as_str() {
            Some(cursor) => after = cursor.into(),
            None => break,
        }
    }
    assert_eq!(runs.len(), 60);
    let mut long = jobs[0].clone();
    long.task.id = "x".repeat(100);
    long.run_id = format!("{}-attempt-2", long.task.id);
    storage::write(
        &c.state_dir
            .join("jobs")
            .join(format!("{}.json", long.task.id)),
        &long,
    )
    .unwrap();
    runtime::people::register_history(&c, std::slice::from_ref(&long)).unwrap();
    assert!(personas::historical_job(&c.state_dir, &long.run_id).is_ok());
    assert!(personas::history(&c, &id, &long.run_id, &shared).is_ok());
    assert!(personas::historical_job(&c.state_dir, "case-00-attempt-1").is_ok());
    for run in [
        "case-00-attempt-3",
        "case-00-attempt-01",
        "../../private-attempt-1",
    ] {
        assert!(personas::historical_job(&c.state_dir, run).is_err());
    }
    assert!(personas::history(&c, &id, "foreign-attempt-1", &shared).is_err());
}

#[test]
fn execution_catalog_exposes_choices_without_executor_secrets() {
    let c:Config = serde_json::from_value(json!({"schema_version":1,"workspace":"/private","state_dir":"/private/state","superpod":"/private/superpod","codex":"PRIVATE_CODEX_PATH","workflow":"/unused","daily_seconds":600,"max_agents":2,"task_timeout_seconds":60,"require_latest":false,"models":{"research":"model-a"},"tools":{},
        "agent_backends":{"bridge":{"kind":"json_process","program":"/PRIVATE_PROGRAM","args":["PRIVATE_ARGUMENT"],"env_allowlist":["PRIVATE_TOKEN"],"capabilities":{"structured_result":true,"read_workspace":true,"write_workspace":false,"tool_execution":false,"desktop":false}}},
        "backend_models":{"bridge":["model-b"]}})).unwrap();
    let catalog = agent_backend::public_inventory(&c);
    assert_eq!(catalog["backends"][0]["id"], "codex");
    assert_eq!(catalog["backends"][1]["models"], json!(["model-b"]));
    assert_eq!(catalog["backends"][1]["verification"], "configured_only");
    assert!(!catalog.to_string().contains("PRIVATE_"));
    let view = project(&job(), &json!({"state":"running"}));
    assert_eq!(view["session_id"], "research-one");
    assert_eq!(view["execution_id"], "research-one-attempt-1");
}
