use super::*;
fn config(root: &Path, memory: &Path) -> Config {
    serde_json::from_value(json!({"schema_version":1,"workspace":root,"state_dir":root.join("state"),"superpod":root.join("superpod"),"codex":"unused","workflow":root.join("workflow"),"daily_seconds":600,"max_agents":2,"task_timeout_seconds":60,"require_latest":false,"models":{"research":"test-model","review":"critic-model","implement":"writer-model"},"tools":{"relay-memory":{"binary":memory,"repository":root,"probe":[]}}})).unwrap()
}
fn task(id: &str, person: &str) -> Task {
    serde_json::from_value(json!({"id":id,"role":"research","repository":"superpod","prompt":"继续研究数字人跨群的连续记忆","persona_id":person})).unwrap()
}
fn create(c: &Config, kind: Kind, name: &str) -> Person {
    change(
        c,
        Change::Create {
            name: name.into(),
            role: "research".into(),
            kind,
            soul: "带着好奇思考，面对反例修正判断。".into(),
            purpose: "研究记忆恢复".into(),
        },
    )
    .unwrap()
}
#[test]
fn all_creation_modes_unique_names_promotion_and_legacy_history_survive() {
    let root = tempfile::tempdir().unwrap();
    let c = config(root.path(), Path::new("/unused"));
    let legacy = c.state_dir.join("dashboard/personas.json");
    storage::write(&legacy, &json!({"old-a":"云舟","old-b":"星河"})).unwrap();
    let directory = directory(&c).unwrap();
    let legacy_id = directory.tasks["old-a"].clone();
    assert_eq!(directory.people[&legacy_id].name, "云舟");
    for kind in [Kind::Fixed, Kind::Temporary, Kind::Research] {
        create(&c, kind, "");
    }
    assert!(
        change(
            &c,
            Change::Create {
                name: "云舟".into(),
                role: "research".into(),
                kind: Kind::Fixed,
                soul: "".into(),
                purpose: "".into()
            }
        )
        .is_err()
    );
    let prior = directory.people[&legacy_id].clone();
    let promoted = change(
        &c,
        Change::Promote {
            id: legacy_id.clone(),
            revision: prior.revision,
        },
    )
    .unwrap();
    assert_eq!(promoted.id, prior.id);
    assert_eq!(promoted.kind, Kind::Fixed);
    assert_eq!(promoted.name, prior.name);
    assert!(
        change(
            &c,
            Change::Update {
                id: legacy_id.clone(),
                revision: prior.revision,
                name: "改名".into(),
                soul: "".into(),
                purpose: "".into()
            }
        )
        .is_err()
    );
    change(
        &c,
        Change::SetDefault {
            role: "research".into(),
            id: legacy_id.clone(),
        },
    )
    .unwrap();
    let after = super::directory(&c).unwrap();
    assert_eq!(after.tasks["old-a"], legacy_id);
    assert_eq!(default_id(&c, "research").unwrap(), legacy_id);
    assert_eq!(
        storage::read::<Value>(&legacy).unwrap(),
        json!({"old-a":"云舟","old-b":"星河"})
    );
    let names: BTreeSet<_> = after.people.values().map(|p| &p.name).collect();
    assert_eq!(names.len(), after.people.len());
}
#[test]
fn invalid_or_duplicate_legacy_directory_is_not_overwritten() {
    let root = tempfile::tempdir().unwrap();
    let c = config(root.path(), Path::new("/unused"));
    let legacy = c.state_dir.join("dashboard/personas.json");
    let value = json!({"old-a":"云舟","old-b":"云舟"});
    storage::write(&legacy, &value).unwrap();
    assert!(directory(&c).is_err());
    assert!(!path(&c).exists());
    assert_eq!(storage::read::<Value>(&legacy).unwrap(), value);
}
#[test]
fn ambiguous_memory_write_is_never_blindly_replayed() {
    let root = tempfile::tempdir().unwrap();
    let c = config(root.path(), Path::new("/bin/false"));
    let p = create(&c, Kind::Temporary, "待核对");
    assert!(remember_note(&c, &p.id, "note-1", "研究线索").is_err());
    let error = remember_note(&c, &p.id, "note-1", "研究线索")
        .unwrap_err()
        .to_string();
    assert!(error.contains("reconciliation"));
    let error = remember_note(&c, &p.id, "note-1", "不同内容")
        .unwrap_err()
        .to_string();
    assert!(error.contains("different input"));
}
#[test]
#[ignore = "requires the exact configured relay-memory CLI via LAB_PERSONA_MEMORY_BINARY"]
fn real_memory_cross_group_recall_isolated_people_frozen_inputs_and_promotion() {
    let binary =
        std::env::var_os("LAB_PERSONA_MEMORY_BINARY").expect("exact relay-memory binary required");
    let root = tempfile::tempdir().unwrap();
    let mut c = config(root.path(), Path::new(&binary));
    let p = create(&c, Kind::Temporary, "知微");
    let other = create(&c, Kind::Research, "明川");
    let before = freeze(&c, &task("group-before", &p.id)).unwrap().unwrap();
    let note =
        "PERSONA_CROSS_GROUP_7281：记忆恢复实验发现并发写回需要独立证据，下一步研究失败后的恢复。";
    let first = remember_note(&c, &p.id, "note-1", note).unwrap();
    let repeated = remember_note(&c, &p.id, "note-1", note).unwrap();
    assert_eq!(first, repeated);
    let after = freeze(&c, &task("group-after", &p.id)).unwrap().unwrap();
    assert!(
        after
            .memory
            .as_ref()
            .unwrap()
            .context
            .contains("PERSONA_CROSS_GROUP_7281")
    );
    assert!(
        !before
            .memory
            .as_ref()
            .unwrap()
            .context
            .contains("PERSONA_CROSS_GROUP_7281")
    );
    let separate = freeze(&c, &task("independent-person", &other.id))
        .unwrap()
        .unwrap();
    assert!(
        !separate
            .memory
            .unwrap()
            .context
            .contains("PERSONA_CROSS_GROUP_7281")
    );
    c.agent_backends.insert(
        "alternate".into(),
        config::BackendSpec::JsonProcess {
            program: "/bin/true".into(),
            args: vec![],
            env_allowlist: vec![],
            capabilities: config::Capabilities {
                structured_result: true,
                read_workspace: true,
                ..Default::default()
            },
        },
    );
    let selected = change(
        &c,
        Change::ConfigureExecution {
            id: p.id.clone(),
            revision: p.revision,
            execution: Some(task::ExecutionPreference {
                backend: Some("alternate".into()),
                model: Some("critic-model".into()),
            }),
        },
    )
    .unwrap();
    let mut review = task("another-backend-and-role", &p.id);
    review.role = "review".into();
    let alternate = freeze(&c, &review).unwrap().unwrap();
    assert_eq!(alternate.id, after.id);
    assert_eq!(alternate.soul, after.soul);
    assert!(
        alternate
            .memory
            .as_ref()
            .unwrap()
            .context
            .contains("PERSONA_CROSS_GROUP_7281")
    );
    assert_eq!(
        alternate.execution.as_ref().unwrap().backend.as_deref(),
        Some("alternate")
    );
    assert!(after.execution.is_none());
    let original = prompt(Some(&after)).unwrap();
    let promoted = change(
        &c,
        Change::Promote {
            id: p.id.clone(),
            revision: selected.revision,
        },
    )
    .unwrap();
    change(
        &c,
        Change::Update {
            id: p.id.clone(),
            revision: promoted.revision,
            name: "知微".into(),
            soul: "新的表达方式".into(),
            purpose: p.purpose.clone(),
        },
    )
    .unwrap();
    assert_eq!(original, prompt(Some(&after)).unwrap());
    let recalled = inspect_memory(&c, &p.id, "记忆恢复实验").unwrap();
    assert_eq!(recalled["stats"]["event_count"], 1);
    assert!(recalled.to_string().contains("PERSONA_CROSS_GROUP_7281"));
    let mut tampered = after;
    tampered
        .memory
        .as_mut()
        .unwrap()
        .context
        .push_str(" altered");
    assert!(prompt(Some(&tampered)).is_err());
    assert_ne!(
        c.state_dir.join("people/memory").join(&p.id),
        c.state_dir.join("task-memory/group-after")
    );
}

#[test]
fn specialty_and_execution_are_independent_of_identity_and_task_roles() {
    let root = tempfile::tempdir().unwrap();
    let mut c = config(root.path(), Path::new("/unused"));
    c.agent_backends.insert(
        "bridge".into(),
        config::BackendSpec::JsonProcess {
            program: "/bin/true".into(),
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
        .insert("bridge".into(), vec!["independent-model".into()]);
    let p = change(
        &c,
        Change::Create {
            name: "知桥".into(),
            role: "designer".into(),
            kind: Kind::Fixed,
            soul: "好奇而审慎".into(),
            purpose: "跨技术研究".into(),
        },
    )
    .unwrap();
    for role in ["research", "review"] {
        change(
            &c,
            Change::SetDefault {
                role: role.into(),
                id: p.id.clone(),
            },
        )
        .unwrap();
        assert_eq!(default_id(&c, role).unwrap(), p.id);
    }
    let choice = task::ExecutionPreference {
        backend: Some("bridge".into()),
        model: Some("independent-model".into()),
    };
    let updated = change(
        &c,
        Change::ConfigureExecution {
            id: p.id.clone(),
            revision: p.revision,
            execution: Some(choice.clone()),
        },
    )
    .unwrap();
    assert_eq!(updated.id, p.id);
    assert_eq!(updated.soul, p.soul);
    assert_eq!(updated.role, "designer");
    assert_eq!(updated.execution, Some(choice.clone()));
    assert!(
        change(
            &c,
            Change::ConfigureExecution {
                id: p.id.clone(),
                revision: p.revision,
                execution: None
            }
        )
        .is_err()
    );
    for invalid in [
        task::ExecutionPreference {
            backend: Some("unknown".into()),
            model: None,
        },
        task::ExecutionPreference {
            backend: Some("codex".into()),
            model: Some("independent-model".into()),
        },
    ] {
        assert!(
            change(
                &c,
                Change::ConfigureExecution {
                    id: p.id.clone(),
                    revision: updated.revision,
                    execution: Some(invalid)
                }
            )
            .is_err()
        );
    }
    assert_eq!(directory(&c).unwrap().people[&p.id].execution, Some(choice));
    let cleared = change(
        &c,
        Change::ConfigureExecution {
            id: p.id.clone(),
            revision: updated.revision,
            execution: None,
        },
    )
    .unwrap();
    assert!(cleared.execution.is_none());
    assert_eq!(directory(&c).unwrap().defaults["review"], p.id);
    // Executor model inventories never create identities or business roles.
    assert!(
        !directory(&c)
            .unwrap()
            .defaults
            .contains_key("independent-model")
    );
}

#[test]
fn unchanged_directory_reads_do_not_replace_the_file() {
    use std::os::unix::fs::MetadataExt;
    let root = tempfile::tempdir().unwrap();
    let c = config(root.path(), Path::new("/unused"));
    directory(&c).unwrap();
    let before = fs::metadata(path(&c)).unwrap();
    for _ in 0..3 {
        directory(&c).unwrap();
    }
    let after = fs::metadata(path(&c)).unwrap();
    assert_eq!(before.ino(), after.ino());
    assert_eq!(before.modified().unwrap(), after.modified().unwrap());
}
