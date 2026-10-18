use super::*;

fn valid_config() -> Config {
    Config {
        schema_version: 1,
        workspace: "/workspace".into(),
        state_dir: "/workspace/lab/.lab".into(),
        superpod: "/workspace/superpod".into(),
        codex: "codex".into(),
        agent_backends: Default::default(),
        multi_agent: None,
        role_backends: Default::default(),
        backend_models: Default::default(),
        workflow: "/tools/workflow".into(),
        daily_seconds: 43200,
        max_agents: 8,
        task_timeout_seconds: 1800,
        models: ["research", "implement", "review"]
            .into_iter()
            .map(|role| (role.into(), "fixture-model".into()))
            .collect(),
        tools: [
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
                Tool {
                    binary: Path::new("/tools").join(name),
                    repository: Path::new("/workspace").join(name),
                    probe: vec!["--version".into()],
                },
            )
        })
        .collect(),
        require_latest: true,
        skills_manifest: Some("/workspace/lab/.lab/latest/skills.json".into()),
    }
}

#[test]
fn accepts_up_to_eight_agents() {
    let mut config = valid_config();
    for count in [1, 3, 8] {
        config.max_agents = count;
        config.validate().unwrap();
    }
}

#[test]
fn rejects_concurrency_outside_one_to_eight() {
    let mut config = valid_config();
    for count in [0, 9] {
        config.max_agents = count;
        assert!(config.validate().is_err(), "accepted {count} agents");
    }
}

#[test]
fn eight_agents_preserve_daily_and_task_budget_limits() {
    let config = valid_config();
    config.validate().unwrap();
    for daily_seconds in [0, 43201] {
        let invalid = Config {
            daily_seconds,
            ..config.clone()
        };
        assert!(invalid.validate().is_err());
    }
    for task_timeout_seconds in [0, 43201] {
        let invalid = Config {
            task_timeout_seconds,
            ..config.clone()
        };
        assert!(invalid.validate().is_err());
    }
    let mut shorter_day = Config {
        daily_seconds: 120,
        task_timeout_seconds: 120,
        ..config
    };
    shorter_day.validate().unwrap();
    shorter_day.task_timeout_seconds = 121;
    assert!(shorter_day.validate().is_err());
}
