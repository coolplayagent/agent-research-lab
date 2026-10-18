use crate::{
    PromotionRequest, PromptRegistration, execute,
    files::read_strict,
    resolve_selection,
    test_support::{example, execute_fixture, prepare_gate, setup},
};
use evolution::{EvaluationContext, PromptRegistry, PromptVersion, Variant};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

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

fn prepare_promotion(root: &Path) -> (PathBuf, PathBuf) {
    fs::create_dir(root.join("evolution")).unwrap();
    assert_eq!(resolve_selection(root, "research").unwrap(), None);
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
    let registry_path = root.join("evolution/prompts.json");
    storage::write(&registry_path, &registry).unwrap();
    let gate_input = prepare_gate(root, "evaluate");
    let mut gate: PromotionRequest = read_strict(&gate_input).unwrap();
    let context_path = root.join("host/context.json");
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
    let receipt_path = root.join("host/evaluation-receipt.json");
    execute("evaluate", &gate_input, Some(&receipt_path)).unwrap();
    let input = root.join("host/promote.json");
    let output = root.join("evolution/selections.json");
    let mut request = example("promote");
    request["candidate_version"] = candidate.clone().into();
    storage::write(&input, &request).unwrap();
    (input, output)
}

#[test]
fn repeated_selection_rechecks_remote_release_and_installed_bytes() {
    let temp = setup();
    let (input, output) = prepare_promotion(temp.path());
    execute_fixture("promote", &input, Some(&output)).unwrap();
    let before = fs::read(&output).unwrap();
    let mutations = [
        (
            temp.path().join("host/current-upstream.json"),
            serde_json::to_vec(&json!({"sha":"e".repeat(40)})).unwrap(),
        ),
        (
            temp.path().join("host/current-release.json"),
            serde_json::to_vec(&json!({"id":2,"tag_name":"v2","draft":false,"prerelease":false}))
                .unwrap(),
        ),
        (
            temp.path().join("host/verified-skill/cli"),
            b"changed runtime".to_vec(),
        ),
    ];
    for (path, changed) in mutations {
        let original = fs::read(&path).unwrap();
        fs::write(&path, changed).unwrap();
        assert!(execute_fixture("promote", &input, Some(&output)).is_err());
        assert_eq!(before, fs::read(&output).unwrap());
        fs::write(&path, original).unwrap();
        assert_eq!(
            execute_fixture("promote", &input, Some(&output)).unwrap()["already_selected"],
            true
        );
    }
}
