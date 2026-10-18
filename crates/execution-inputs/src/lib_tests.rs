use super::*;

fn knowledge_fixture(content: &str) -> (tempfile::TempDir, RepositoryInput) {
    let root = tempfile::tempdir().unwrap();
    git(root.path(), &["init", "-q"]).unwrap();
    git(root.path(), &["config", "user.name", "fixture"]).unwrap();
    git(
        root.path(),
        &["config", "user.email", "fixture@example.invalid"],
    )
    .unwrap();
    for path in KNOWLEDGE_ENTRYPOINTS {
        let destination = root.path().join(path);
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::write(destination, content).unwrap();
    }
    git(root.path(), &["add", "knowledge"]).unwrap();
    git(
        root.path(),
        &["-c", "commit.gpgsign=false", "commit", "-qm", "knowledge"],
    )
    .unwrap();
    let input = RepositoryInput {
        checkout: root.path().into(),
        upstream: RemoteSnapshot {
            repository: "stevetdp/superpod".into(),
            default_branch: "main".into(),
            commit: git(root.path(), &["rev-parse", "HEAD"]).unwrap(),
            checked_at: "2026-10-10T00:00:00Z".into(),
        },
    };
    (root, input)
}

#[test]
fn reviewed_insights_require_a_pinned_external_regular_blob() {
    let (root, mut input) = knowledge_fixture("External knowledge");
    assert!(reviewed_insights(&input).is_err());
    let path = root.path().join(REVIEWED_INSIGHTS_PATH);
    let content = include_str!("fixtures/knowledge.md");
    fs::write(&path, content).unwrap();
    assert!(reviewed_insights(&input).is_err());
    git(root.path(), &["add", REVIEWED_INSIGHTS_PATH]).unwrap();
    git(
        root.path(),
        &[
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-qm",
            "reviewed insights",
        ],
    )
    .unwrap();
    input.upstream.commit = git(root.path(), &["rev-parse", "HEAD"]).unwrap();
    fs::write(&path, "uncommitted changes").unwrap();
    assert_eq!(reviewed_insights(&input).unwrap(), content);
    fs::remove_file(&path).unwrap();
    std::os::unix::fs::symlink("../../methodology/evidence-method.md", &path).unwrap();
    git(root.path(), &["add", REVIEWED_INSIGHTS_PATH]).unwrap();
    git(
        root.path(),
        &[
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-qm",
            "symlink fixture",
        ],
    )
    .unwrap();
    input.upstream.commit = git(root.path(), &["rev-parse", "HEAD"]).unwrap();
    assert!(reviewed_insights(&input).is_err());
}

#[test]
fn shared_knowledge_reads_exact_committed_bytes_not_dirty_files() {
    let content = include_str!("fixtures/knowledge.md");
    let (root, input) = knowledge_fixture(content);
    fs::write(
        root.path().join(KNOWLEDGE_ENTRYPOINTS[0]),
        "uncommitted claim",
    )
    .unwrap();
    let pack = shared_knowledge(&input).unwrap();
    assert_eq!(pack.superpod_commit, input.upstream.commit);
    assert_eq!(pack.documents.len(), KNOWLEDGE_ENTRYPOINTS.len());
    for doc in &pack.documents {
        assert_eq!(doc.excerpt, content);
        assert_eq!(doc.source_bytes, content.len());
        assert_eq!(doc.source_sha256, storage::digest(content.as_bytes()));
        assert_eq!(doc.excerpt_sha256, doc.source_sha256);
        assert!(!doc.truncated);
    }
    let mut inputs: ExecutionInputs = serde_json::from_value(json!({
        "repositories":{"superpod":input},
        "skills":{"schema_version":1,"skills":[]},
        "insights":"Existing hypothesis.","insights_sha256":"a".repeat(64)
    }))
    .unwrap();
    let historical_context = prompt_context(&inputs);
    let historical_cohort = cohort(&inputs);
    inputs.shared_knowledge = Some(pack);
    let context = prompt_context(&inputs);
    assert!(context.starts_with(&historical_context));
    assert!(context.contains(include_str!("fixtures/title.txt")));
    assert!(context.contains("Host knowledge adapters"));
    assert_ne!(cohort(&inputs), historical_cohort);
    inputs.shared_knowledge.as_mut().unwrap().documents[0]
        .excerpt
        .push('!');
    assert_ne!(
        inputs.shared_knowledge.as_ref().unwrap(),
        &shared_knowledge(&inputs.repositories["superpod"]).unwrap()
    );
}

#[test]
fn shared_knowledge_bounds_unicode_and_distinguishes_full_source_digest() {
    let content = include_str!("fixtures/character.txt").repeat(KNOWLEDGE_EXCERPT_LIMIT);
    let (_root, input) = knowledge_fixture(&content);
    let pack = shared_knowledge(&input).unwrap();
    for doc in pack.documents {
        assert!(doc.truncated);
        assert!(doc.excerpt.len() <= KNOWLEDGE_EXCERPT_LIMIT);
        assert_eq!(doc.source_bytes, content.len());
        assert_eq!(doc.source_sha256, storage::digest(content.as_bytes()));
        assert_eq!(doc.excerpt_sha256, storage::digest(doc.excerpt.as_bytes()));
        assert_ne!(doc.excerpt_sha256, doc.source_sha256);
    }
}

#[test]
fn shared_knowledge_rejects_symlinks_missing_and_oversized_entries() {
    let (root, mut input) = knowledge_fixture("Bounded evidence\n");
    let path = root.path().join(KNOWLEDGE_ENTRYPOINTS[0]);
    fs::remove_file(&path).unwrap();
    git(root.path(), &["add", "-A"]).unwrap();
    git(
        root.path(),
        &["-c", "commit.gpgsign=false", "commit", "-qm", "missing"],
    )
    .unwrap();
    input.upstream.commit = git(root.path(), &["rev-parse", "HEAD"]).unwrap();
    assert!(
        shared_knowledge(&input)
            .unwrap_err()
            .to_string()
            .contains("regular file")
    );
    std::os::unix::fs::symlink("../AGENTS.md", &path).unwrap();
    git(root.path(), &["add", "-A"]).unwrap();
    git(
        root.path(),
        &["-c", "commit.gpgsign=false", "commit", "-qm", "symlink"],
    )
    .unwrap();
    input.upstream.commit = git(root.path(), &["rev-parse", "HEAD"]).unwrap();
    assert!(shared_knowledge(&input).is_err());
    fs::remove_file(&path).unwrap();
    fs::write(&path, vec![b'a'; KNOWLEDGE_FILE_LIMIT + 1]).unwrap();
    git(root.path(), &["add", "-A"]).unwrap();
    git(
        root.path(),
        &["-c", "commit.gpgsign=false", "commit", "-qm", "oversized"],
    )
    .unwrap();
    input.upstream.commit = git(root.path(), &["rev-parse", "HEAD"]).unwrap();
    assert!(
        shared_knowledge(&input)
            .unwrap_err()
            .to_string()
            .contains("1 MiB")
    );
}

#[test]
fn legacy_skill_receipts_keep_their_cohort_until_a_source_proof_is_added() {
    let skill = json!({
        "name":"workflow-cli","repository":"coolplayagent/workflow-cli",
        "installed_path":"/fixture/skill","runtime_path":"/fixture/skill/workflow",
        "tree_sha256":"a".repeat(64),"runtime_sha256":"b".repeat(64),
        "release_tag":"v0.2.0","release_id":42,"source_commit":null,
        "checked_at":"2026-10-09T00:00:00Z"
    });
    let mut inputs: ExecutionInputs = serde_json::from_value(json!({
        "repositories":{},"skills":{"schema_version":1,"skills":[skill]},
        "insights":"Existing evidence.","insights_sha256":"c".repeat(64)
    }))
    .unwrap();
    let legacy_identity = json!({"workflow-cli":{
        "repository":"coolplayagent/workflow-cli",
        "installed_path":"/fixture/skill","runtime_path":"/fixture/skill/workflow",
        "tree":"a".repeat(64),"runtime":"b".repeat(64),
        "release_tag":"v0.2.0","release_id":42,"source_commit":null
    }});
    let legacy_cohort = storage::digest(
        json!({
            "repositories":{},"skills":legacy_identity,"insights":"c".repeat(64)
        })
        .to_string()
        .as_bytes(),
    )[..12]
        .to_owned();
    assert_eq!(skill_identity(&inputs.skills), legacy_identity);
    assert_eq!(cohort(&inputs), legacy_cohort);
    assert!(
        serde_json::to_value(&inputs.skills.skills[0])
            .unwrap()
            .get("source_qualitygate")
            .is_none()
    );
    inputs.skills.skills[0].source_qualitygate = Some(freshness::SourceQualitygate {
        path: "/fixture/source-gate.json".into(),
        sha256: "d".repeat(64),
    });
    let first_proof_cohort = cohort(&inputs);
    assert_ne!(first_proof_cohort, legacy_cohort);
    inputs.skills.skills[0]
        .source_qualitygate
        .as_mut()
        .unwrap()
        .sha256 = "e".repeat(64);
    assert_ne!(cohort(&inputs), first_proof_cohort);
}

#[test]
fn cohort_changes_with_code_or_research_but_not_check_time() {
    let snapshot = RemoteSnapshot {
        repository: "coolplayagent/agent-research-lab".into(),
        default_branch: "main".into(),
        commit: "a".repeat(40),
        checked_at: "2026-10-09T00:00:00Z".into(),
    };
    let mut input = ExecutionInputs {
        repositories: BTreeMap::from([(
            "agent-research-lab".into(),
            RepositoryInput {
                upstream: snapshot,
                checkout: "/fixture".into(),
            },
        )]),
        skills: SkillsManifest {
            schema_version: 1,
            skills: vec![],
        },
        insights: "Hypothesis H1: test, do not assume.".into(),
        insights_sha256: storage::digest(b"Hypothesis H1: test, do not assume."),
        shared_knowledge: None,
    };
    let initial = cohort(&input);
    input
        .repositories
        .get_mut("agent-research-lab")
        .unwrap()
        .upstream
        .checked_at = "2026-10-10T00:00:00Z".into();
    assert_eq!(initial, cohort(&input));
    assert!(prompt_context(&input).contains("Hypothesis H1"));
    input
        .repositories
        .get_mut("agent-research-lab")
        .unwrap()
        .upstream
        .commit = "b".repeat(40);
    assert_ne!(initial, cohort(&input));
    let next = cohort(&input);
    input.insights_sha256 = storage::digest(b"counterexample H1");
    assert_ne!(next, cohort(&input));
}

mod complete_identity_tests {
    use super::*;
    #[test]
    fn identity_ignores_only_two_typed_observation_locations() {
        let original = serde_json::json!({
            "repositories":{"superpod":{"upstream":{"repository":"fixture/source","default_branch":"main","commit":"a".repeat(40),"checked_at":"observed"},"checkout":"/owned/source"}},
            "skills":{"schema_version":1,"skills":[{"name":"owned","repository":"fixture/source","installed_path":"/owned/skill","runtime_path":"/owned/runtime","tree_sha256":"b".repeat(64),"runtime_sha256":"c".repeat(64),"release_tag":null,"release_id":null,"source_commit":"a".repeat(40),"source_qualitygate":{"path":"/owned/proof","sha256":"d".repeat(64)},"checked_at":"observed"}]},
            "insights":"checked_at is literal knowledge", "insights_sha256":"insights",
            "shared_knowledge":{"superpod_commit":"a".repeat(40),"documents":[{"path":"knowledge/index.md","source_sha256":"source","source_bytes":42,"excerpt":"checked_at original","excerpt_sha256":"excerpt","truncated":false}]}
        });
        let input: ExecutionInputs = serde_json::from_value(original.clone()).unwrap();
        let baseline = identity(&input).unwrap();
        for pointer in [
            "/repositories/superpod/upstream/checked_at",
            "/skills/skills/0/checked_at",
        ] {
            let mut changed = original.clone();
            *changed.pointer_mut(pointer).unwrap() = json!("new observation");
            assert_eq!(
                identity(&serde_json::from_value(changed).unwrap()).unwrap(),
                baseline
            );
        }
        for pointer in [
            "/repositories/superpod/upstream/repository",
            "/repositories/superpod/upstream/default_branch",
            "/repositories/superpod/upstream/commit",
            "/repositories/superpod/checkout",
            "/skills/skills/0/installed_path",
            "/skills/skills/0/runtime_path",
            "/skills/skills/0/tree_sha256",
            "/skills/skills/0/runtime_sha256",
            "/skills/skills/0/source_commit",
            "/skills/skills/0/source_qualitygate/path",
            "/skills/skills/0/source_qualitygate/sha256",
            "/insights",
            "/insights_sha256",
            "/shared_knowledge/documents/0/path",
            "/shared_knowledge/documents/0/source_sha256",
            "/shared_knowledge/documents/0/excerpt",
            "/shared_knowledge/documents/0/excerpt_sha256",
        ] {
            let mut changed = original.clone();
            *changed.pointer_mut(pointer).unwrap() = json!("changed checked_at literal");
            assert_ne!(
                identity(&serde_json::from_value(changed).unwrap()).unwrap(),
                baseline,
                "{pointer}"
            );
        }
    }
}
