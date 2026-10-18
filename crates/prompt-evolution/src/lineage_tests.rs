use super::*;
fn register(r: &mut PromptRegistry, name: &str, parents: Vec<String>) -> String {
    r.register(
        PromptVersion::new(PromptDefinition {
            parent_versions: parents,
            role: "research".into(),
            task_kind: "coding".into(),
            content: name.into(),
            change_reason: name.into(),
            failure_conditions: vec!["regression".into()],
        })
        .unwrap(),
    )
    .unwrap()
}
fn binding(version: &str) -> SnapshotBinding {
    SnapshotBinding {
        source_commit: "a".repeat(40),
        prompt_version: version.into(),
        policy_version: "b".repeat(40),
        superpod_commit: "c".repeat(40),
        configuration_digest: "d".repeat(64),
    }
}
#[test]
fn descendant_evidence_survives_recombination_without_double_counting_or_promotion() {
    let mut registry = PromptRegistry::default();
    let a = register(&mut registry, "a", vec![]);
    let b = register(&mut registry, "b", vec![a.clone()]);
    let c = register(&mut registry, "c", vec![a.clone()]);
    let d = register(&mut registry, "d", vec![b.clone(), c.clone()]);
    let observation = TrialObservation {
        trial_id: "shared-case".into(),
        task_family: "family".into(),
        partition: DatasetPartition::Development,
        variant: Variant::Candidate,
        snapshot: binding(&d),
        success: true,
        duration_ms: 10,
        tokens: None,
        critical_regression: false,
    };
    let mut study = Study {
        task_kind: "coding".into(),
        policy_version: "b".repeat(40),
        superpod_commit: "c".repeat(40),
        expected: BTreeMap::from([(d.clone(), binding(&d))]),
        observations: vec![observation],
        pending_evaluations: BTreeMap::from([(b.clone(), 1)]),
        pending_expansions: BTreeMap::new(),
        evaluation_budget: 10,
    };
    let report = diagnose(&registry, &study).unwrap();
    let root = report.nodes.iter().find(|n| n.version == a).unwrap();
    assert_eq!(root.clade_trials, 1);
    assert_eq!(root.descendant_versions, 3);
    assert_eq!(root.trials, 0);
    assert_eq!(root.tokens, None);
    assert!(!report.promotion_authorized);
    assert_eq!(report.budget_remaining, 8);
    study.observations.push(study.observations[0].clone());
    assert!(diagnose(&registry, &study).is_err());
    study.observations.pop();
    study.observations[0].partition = DatasetPartition::Holdout;
    assert!(diagnose(&registry, &study).is_err());
    study.observations[0].partition = DatasetPartition::Development;
    study.observations[0].snapshot.source_commit = "e".repeat(40);
    assert!(diagnose(&registry, &study).is_err());
}
