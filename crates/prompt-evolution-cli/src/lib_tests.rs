use super::*;

#[test]
fn lineage_receipt_recomputes_evidence_and_preserves_registry_aliases() {
    let temp = setup();
    let input = temp.path().join("host/register.json");
    let registry = temp.path().join("host/registry.json");
    storage::write(&input, &example("prompt-register")).unwrap();
    execute("prompt-register", &input, Some(&registry)).unwrap();
    let request = temp.path().join("host/lineage.json");
    let study = evolution::lineage::Study {
        task_kind: "long_horizon".into(),
        policy_version: "a".repeat(40),
        superpod_commit: "b".repeat(40),
        expected: BTreeMap::new(),
        observations: vec![],
        pending_evaluations: BTreeMap::new(),
        pending_expansions: BTreeMap::new(),
        evaluation_budget: 10,
    };
    let output = temp.path().join("host/report.json");
    storage::write(
        &request,
        &LineageRequest {
            registry_file: "registry.json".into(),
            study,
        },
    )
    .unwrap();
    execute("lineage", &request, Some(&output)).unwrap();
    let mut receipt: LineageReceipt = storage::read(&output).unwrap();
    receipt.validate().unwrap();
    receipt.report.budget_remaining = 0;
    assert!(receipt.validate().is_err());
    let alias = temp.path().join("host/alias.json");
    std::os::unix::fs::symlink(&registry, &alias).unwrap();
    assert!(execute("lineage", &request, Some(&alias)).is_err());
    read_strict::<PromptRegistry>(&registry)
        .unwrap()
        .validate()
        .unwrap();
}

fn mock_remote(root: &Path) -> RemoteClient {
    use std::os::unix::fs::PermissionsExt;
    let head = root.join("current-upstream.json");
    if !head.exists() {
        storage::write(&head, &json!({"sha":"a".repeat(40)})).unwrap();
    }
    let gh = root.join("fake-gh");
    let quoted = head.to_string_lossy().replace('\'', "'\\''");
    let release = root.join("current-release.json");
    if !release.exists() {
        storage::write(
            &release,
            &json!({"id":1,"tag_name":"v1","draft":false,"prerelease":false}),
        )
        .unwrap();
    }
    let release = release.to_string_lossy().replace('\'', "'\\''");
    fs::write(&gh, format!("#!/bin/sh\nset -eu\ncase \"$1\" in\nrepo) printf '%s\\n' '{{\"nameWithOwner\":\"coolplayagent/example\",\"defaultBranchRef\":{{\"name\":\"main\"}}}}';;\napi) case \"$2\" in */releases/latest) cat '{release}';; *) cat '{quoted}';; esac;;\n*) exit 92;;\nesac\n")).unwrap();
    fs::set_permissions(&gh, fs::Permissions::from_mode(0o755)).unwrap();
    RemoteClient {
        git: PathBuf::from("git"),
        gh,
        timeout: std::time::Duration::from_secs(2),
    }
}

fn execute_fixture(action: &str, input: &Path, output: Option<&Path>) -> Result<Value> {
    let host = input.parent().unwrap();
    execute_with_client(action, input, output, &mock_remote(host))
}

fn example(name: &str) -> Value {
    serde_json::from_str(match name {
        "prompt-register" => include_str!("../../../examples/evolution/prompt-register.json"),
        "context" => include_str!("../../../examples/evolution/context.json"),
        "evaluate" => include_str!("../../../examples/evolution/evaluate.json"),
        "novelty" => include_str!("../../../examples/evolution/novelty.json"),
        "exploration" => include_str!("../../../examples/evolution/exploration.json"),
        "rule-proposal" => include_str!("../../../examples/evolution/rule-proposal.json"),
        "promote" => include_str!("../../../examples/evolution/promote.json"),
        _ => panic!("unknown fixture"),
    })
    .unwrap()
}

fn setup() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    for name in ["host", "holdout", "optimizer", "superpod", "memory"] {
        fs::create_dir(temp.path().join(name)).unwrap();
    }
    temp
}

fn prepare_gate(temp: &Path, action: &str) -> PathBuf {
    let mut request = example(action);
    request["context_file"] = "context.json".into();
    request["holdout_isolation"] = json!({
        "private_root":"../holdout","optimizer_roots":["../optimizer"],
        "shared_knowledge_roots":["../superpod"],"shared_memory_roots":["../memory"]
    });
    let installed = temp.join("host/verified-skill");
    fs::create_dir_all(&installed).unwrap();
    fs::write(
        installed.join("SKILL.md"),
        "---\nname: verified-skill\n---\nFixture",
    )
    .unwrap();
    fs::write(installed.join("cli"), "verified runtime").unwrap();
    let manifest = temp.join("host/skills-manifest.json");
    storage::write(&manifest, &json!({"schema_version":1,"skills":[{"name":"verified-skill","repository":"coolplayagent/example","installed_path":installed,"runtime_path":installed.join("cli"),"tree_sha256":freshness::skill_tree_digest(&installed).unwrap(),"runtime_sha256":delivery::file_sha256(&installed.join("cli")).unwrap(),"release_tag":"v1","release_id":1,"source_commit":null,"checked_at":"2026-10-09T00:00:00Z"}]})).unwrap();
    request["skills_manifest"] =
        json!({"path":"skills-manifest.json","sha256":delivery::file_sha256(&manifest).unwrap()});
    storage::write(&temp.join("host/context.json"), &example("context")).unwrap();
    let path = temp.join(format!("host/{action}.json"));
    storage::write(&path, &request).unwrap();
    path
}

#[test]
fn example_prompt_registration_is_idempotent_and_tampering_fails() {
    let temp = setup();
    let input = temp.path().join("host/request.json");
    let output = temp.path().join("host/registry.json");
    storage::write(&input, &example("prompt-register")).unwrap();
    assert!(execute("prompt-register", &input, None).is_err());
    let result = execute("prompt-register", &input, Some(&output)).unwrap();
    assert_eq!(result["already_registered"], false);
    assert_eq!(
        execute("prompt-register", &input, Some(&output)).unwrap()["already_registered"],
        true
    );
    let registry: PromptRegistry = read_strict(&output).unwrap();
    registry.validate().unwrap();
    let version = result["version"].as_str().unwrap();
    let mut changed: Value = storage::read(&output).unwrap();
    changed["versions"][version]["definition"]["content"] = "tampered".into();
    storage::write(&output, &changed).unwrap();
    assert!(execute("prompt-register", &input, Some(&output)).is_err());
}

#[test]
fn examples_evaluate_and_novelty_produce_recomputable_receipts() {
    let temp = setup();
    for action in ["evaluate", "novelty"] {
        let input = prepare_gate(temp.path(), action);
        let output = temp.path().join(format!("host/{action}-receipt.json"));
        let result = execute(action, &input, Some(&output)).unwrap();
        assert_eq!(result["decision"]["approved"], true);
        let receipt: EvaluationReceipt = read_strict(&output).unwrap();
        receipt.validate().unwrap();
        let mut tampered = receipt;
        tampered.decision.approved = false;
        assert!(tampered.validate().is_err());
    }
}

#[test]
fn candidates_cannot_supply_approval_or_host_gate_files() {
    let temp = setup();
    let input = prepare_gate(temp.path(), "evaluate");
    let mut request: Value = storage::read(&input).unwrap();
    request["observations"][0]["approved"] = true.into();
    storage::write(&input, &request).unwrap();
    assert!(execute("evaluate", &input, None).is_err());
    let input = prepare_gate(temp.path(), "evaluate");
    let agent_output = temp.path().join("optimizer/receipt.json");
    assert!(execute("evaluate", &input, Some(&agent_output)).is_err());
    let mut request: Value = storage::read(&input).unwrap();
    storage::write(
        &temp.path().join("optimizer/context.json"),
        &example("context"),
    )
    .unwrap();
    request["context_file"] = "../optimizer/context.json".into();
    storage::write(&input, &request).unwrap();
    assert!(execute("evaluate", &input, None).is_err());
}

#[test]
fn exploration_persists_raw_budget_and_rejects_replay_without_partial_writes() {
    let temp = setup();
    let gate = prepare_gate(temp.path(), "evaluate");
    let receipt = temp.path().join("host/evaluation-receipt.json");
    execute("evaluate", &gate, Some(&receipt)).unwrap();
    let mut request = example("exploration");
    request["holdout_isolation"] = serde_json::to_value(
        read_strict::<PromotionRequest>(&gate)
            .unwrap()
            .holdout_isolation,
    )
    .unwrap();
    request["experiments"] =
        json!([{"experiment_id":"first","evaluation_receipt":"evaluation-receipt.json"}]);
    let input = temp.path().join("host/exploration.json");
    let output = temp.path().join("host/budget.json");
    storage::write(&input, &request).unwrap();
    execute_fixture("exploration", &input, Some(&output)).unwrap();
    let budget: ExplorationBudget = read_strict(&output).unwrap();
    assert_eq!(budget.pending_count(), 1);
    assert_eq!(budget.share_percent(), 25);
    let before = fs::read(&output).unwrap();
    request["experiments"] = json!([
        {"experiment_id":"second","evaluation_receipt":"evaluation-receipt.json"},
        {"experiment_id":"first","evaluation_receipt":"evaluation-receipt.json"}
    ]);
    storage::write(&input, &request).unwrap();
    assert!(execute_fixture("exploration", &input, Some(&output)).is_err());
    assert_eq!(before, fs::read(&output).unwrap());
}

#[test]
fn rule_example_stays_quarantined() {
    let temp = setup();
    let input = temp.path().join("host/rule.json");
    storage::write(&input, &example("rule-proposal")).unwrap();
    let result = execute("rule-proposal", &input, None).unwrap();
    assert_eq!(result["quarantined"], true);
    assert_eq!(result["adopted"], false);
}

#[test]
fn promotion_activates_only_bound_candidate_and_is_idempotent() {
    let temp = setup();
    fs::create_dir(temp.path().join("evolution")).unwrap();
    assert_eq!(resolve_selection(temp.path(), "research").unwrap(), None);
    let mut registry = PromptRegistry::default();
    let base_definition: PromptRegistration =
        serde_json::from_value(example("prompt-register")).unwrap();
    let baseline = registry
        .register(PromptVersion::new(base_definition.definition.clone()).unwrap())
        .unwrap();
    let mut definition = base_definition.definition.clone();
    definition.parent_versions = vec![baseline.clone()];
    definition
        .content
        .push_str(" Always run a falsification experiment.");
    let candidate = registry
        .register(PromptVersion::new(definition).unwrap())
        .unwrap();
    let registry_path = temp.path().join("evolution/prompts.json");
    storage::write(&registry_path, &registry).unwrap();
    let gate_input = prepare_gate(temp.path(), "evaluate");
    let mut gate: PromotionRequest = read_strict(&gate_input).unwrap();
    let context_path = temp.path().join("host/context.json");
    let mut context: EvaluationContext = read_strict(&context_path).unwrap();
    context.baseline.prompt_version = baseline.clone();
    context.candidate.prompt_version = candidate.clone();
    for trial in &mut gate.observations {
        trial.snapshot = if trial.variant == Variant::Baseline {
            context.baseline.clone()
        } else {
            context.candidate.clone()
        };
    }
    storage::write(&context_path, &context).unwrap();
    storage::write(&gate_input, &gate).unwrap();
    let receipt_path = temp.path().join("host/evaluation-receipt.json");
    execute("evaluate", &gate_input, Some(&receipt_path)).unwrap();
    let input = temp.path().join("host/promote.json");
    let output = temp.path().join("evolution/selections.json");
    let mut request = example("promote");
    request["candidate_version"] = candidate.clone().into();
    storage::write(&input, &request).unwrap();
    // Unchanged host context + a passing receipt are both stale once the
    // remote default advances; they must not create a selection.
    storage::write(
        &temp.path().join("host/current-upstream.json"),
        &json!({"sha":"e".repeat(40)}),
    )
    .unwrap();
    assert!(execute_fixture("promote", &input, Some(&output)).is_err());
    assert!(!output.exists());
    storage::write(
        &temp.path().join("host/current-upstream.json"),
        &json!({"sha":"a".repeat(40)}),
    )
    .unwrap();
    storage::write(
        &temp.path().join("host/current-release.json"),
        &json!({"id":2,"tag_name":"v2","draft":false,"prerelease":false}),
    )
    .unwrap();
    assert!(execute_fixture("promote", &input, Some(&output)).is_err());
    assert!(!output.exists());
    storage::write(
        &temp.path().join("host/current-release.json"),
        &json!({"id":1,"tag_name":"v1","draft":false,"prerelease":false}),
    )
    .unwrap();
    let result = execute_fixture("promote", &input, Some(&output)).unwrap();
    assert_eq!(result["already_selected"], false);
    assert_eq!(
        resolve_selection(temp.path(), "research").unwrap(),
        Some(candidate.clone())
    );
    let before = fs::read(&output).unwrap();
    assert_eq!(
        execute_fixture("promote", &input, Some(&output)).unwrap()["already_selected"],
        true
    );
    assert_eq!(before, fs::read(&output).unwrap());

    // A valid passing receipt for a new candidate against the previous
    // baseline cannot displace the strategy that has since been selected.
    let mut third = base_definition.definition;
    third.parent_versions = vec![baseline];
    third.content.push_str(" Another independent candidate.");
    let third = registry
        .register(PromptVersion::new(third).unwrap())
        .unwrap();
    storage::write(&registry_path, &registry).unwrap();
    context.candidate.prompt_version = third.clone();
    for trial in &mut gate.observations {
        if trial.variant == Variant::Candidate {
            trial.snapshot = context.candidate.clone();
        }
    }
    storage::write(&context_path, &context).unwrap();
    storage::write(&gate_input, &gate).unwrap();
    execute("evaluate", &gate_input, Some(&receipt_path)).unwrap();
    request["candidate_version"] = third.into();
    storage::write(&input, &request).unwrap();
    assert!(execute_fixture("promote", &input, Some(&output)).is_err());
    assert_eq!(before, fs::read(&output).unwrap());
    assert_eq!(
        resolve_selection(temp.path(), "research").unwrap(),
        Some(candidate)
    );
}

#[test]
fn promotion_rejects_tampered_approval_and_different_registered_candidate() {
    let temp = setup();
    let gate_input = prepare_gate(temp.path(), "evaluate");
    let receipt_path = temp.path().join("host/evaluation-receipt.json");
    execute("evaluate", &gate_input, Some(&receipt_path)).unwrap();
    fs::create_dir(temp.path().join("evolution")).unwrap();
    storage::write(
        &temp.path().join("evolution/prompts.json"),
        &PromptRegistry::default(),
    )
    .unwrap();
    let input = temp.path().join("host/promote.json");
    storage::write(&input, &example("promote")).unwrap();
    let output = temp.path().join("evolution/selections.json");
    assert!(execute_fixture("promote", &input, Some(&output)).is_err());
    let mut receipt: Value = storage::read(&receipt_path).unwrap();
    receipt["decision"]["approved"] = false.into();
    storage::write(&receipt_path, &receipt).unwrap();
    assert!(execute_fixture("promote", &input, Some(&output)).is_err());
    assert!(!output.exists());
}

#[test]
fn old_unbound_receipts_stay_readable_but_cannot_authorize_new_gates() {
    let temp = setup();
    let input = prepare_gate(temp.path(), "evaluate");
    let output = temp.path().join("host/receipt.json");
    execute("evaluate", &input, Some(&output)).unwrap();
    let mut old: Value = storage::read(&output).unwrap();
    old["schema_version"] = 1.into();
    old.as_object_mut().unwrap().remove("upstream");
    storage::write(&output, &old).unwrap();
    let old: EvaluationReceipt = read_strict(&output).unwrap();
    assert!(old.upstream.is_none());
    assert!(old.validate().is_err());
}
