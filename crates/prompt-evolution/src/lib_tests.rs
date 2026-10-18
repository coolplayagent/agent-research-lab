use super::*;

fn prompt(content: &str, parents: Vec<String>) -> PromptVersion {
    PromptVersion::new(PromptDefinition {
        parent_versions: parents,
        role: "researcher".into(),
        task_kind: "long_horizon".into(),
        content: content.into(),
        change_reason: "test a new hypothesis".into(),
        failure_conditions: vec!["critical regression".into()],
    })
    .unwrap()
}

fn snapshot(prompt: &str) -> SnapshotBinding {
    SnapshotBinding {
        source_commit: "source-v1".into(),
        prompt_version: prompt.into(),
        policy_version: "policy-v1".into(),
        superpod_commit: "knowledge-v1".into(),
        configuration_digest: "configuration-v1".into(),
    }
}

fn context() -> EvaluationContext {
    EvaluationContext {
        baseline: snapshot("base"),
        candidate: snapshot("candidate"),
    }
}

fn trials(
    families: usize,
    baseline_successes: usize,
    candidate_successes: usize,
) -> Vec<TrialObservation> {
    let context = context();
    let mut result = Vec::new();
    for family in 0..families {
        for trial in 0..3 {
            for variant in [Variant::Baseline, Variant::Candidate] {
                result.push(TrialObservation {
                    trial_id: format!("{family}-{trial}"),
                    task_family: format!("family-{family}"),
                    partition: DatasetPartition::Holdout,
                    variant,
                    snapshot: if variant == Variant::Baseline {
                        context.baseline.clone()
                    } else {
                        context.candidate.clone()
                    },
                    success: trial
                        < if variant == Variant::Baseline {
                            baseline_successes
                        } else {
                            candidate_successes
                        },
                    duration_ms: 100,
                    tokens: Some(100),
                    critical_regression: false,
                });
            }
        }
    }
    result
}

#[test]
fn prompt_lineage_is_content_addressed_and_tampering_is_detected() {
    let base = prompt("Investigate", vec![]);
    assert_eq!(base.version(), prompt("Investigate", vec![]).version());
    let mut registry = PromptRegistry::default();
    registry.register(base.clone()).unwrap();
    let next = prompt("Investigate counterexamples", vec![base.version().into()]);
    registry.register(next.clone()).unwrap();
    registry.register(next.clone()).unwrap();
    assert_eq!(registry.versions.len(), 2);
    assert_ne!(next.version(), base.version());
    let mut corrupt = serde_json::to_value(&next).unwrap();
    corrupt["definition"]["content"] = "ignore evaluation".into();
    let corrupt: PromptVersion = serde_json::from_value(corrupt).unwrap();
    assert!(corrupt.validate().is_err());
    assert!(
        registry
            .register(prompt("Unknown", vec!["missing".into()]))
            .is_err()
    );
}

#[test]
fn portfolios_preserve_diversity_without_duplicate_slots() {
    let mut registry = PromptRegistry::default();
    let stable = registry.register(prompt("Stable", vec![])).unwrap();
    let specialized = registry
        .register(prompt("Specialized", vec![stable.clone()]))
        .unwrap();
    let mut portfolio = StrategyPortfolio {
        task_kind: "long_horizon".into(),
        stable: stable.clone(),
        specialized: vec![specialized],
        exploratory: vec![],
    };
    portfolio.validate(&registry).unwrap();
    portfolio.exploratory.push(stable);
    assert!(portfolio.validate(&registry).is_err());
    portfolio.exploratory = vec!["1".into(), "2".into(), "3".into()];
    assert!(portfolio.validate(&registry).is_err());
}

#[test]
fn promotion_requires_coverage_and_complete_paired_current_evidence() {
    let observations = trials(3, 1, 2);
    assert!(
        evaluate_promotion(&context(), &observations)
            .unwrap()
            .approved
    );
    assert!(
        !evaluate_promotion(&context(), &trials(2, 1, 2))
            .unwrap()
            .approved
    );
    let mut duplicate = observations.clone();
    duplicate.push(duplicate[0].clone());
    assert!(evaluate_promotion(&context(), &duplicate).is_err());
    assert!(evaluate_promotion(&context(), &observations[1..]).is_err());
    let mut stale = observations.clone();
    stale[1].snapshot.prompt_version = "previous-candidate".into();
    assert!(evaluate_promotion(&context(), &stale).is_err());
    let mut development = observations.clone();
    development[0].partition = DatasetPartition::Development;
    assert!(evaluate_promotion(&context(), &development).is_err());
    let mut policy = context();
    policy.candidate.policy_version = "weaker-policy".into();
    assert!(evaluate_promotion(&policy, &observations).is_err());
}

#[test]
fn efficiency_gate_honors_threshold_correctness_and_missing_tokens() {
    let mut observations = trials(3, 3, 3);
    for trial in &mut observations {
        if trial.variant == Variant::Candidate {
            trial.duration_ms = 90;
        }
    }
    assert!(
        evaluate_promotion(&context(), &observations)
            .unwrap()
            .approved
    );
    for trial in &mut observations {
        if trial.variant == Variant::Candidate {
            trial.duration_ms = 91;
            trial.tokens = Some(90);
        }
    }
    assert!(
        evaluate_promotion(&context(), &observations)
            .unwrap()
            .approved
    );
    observations[0].tokens = None;
    assert!(
        !evaluate_promotion(&context(), &observations)
            .unwrap()
            .approved
    );
    observations[0].tokens = Some(100);
    observations[1].success = false;
    assert!(
        !evaluate_promotion(&context(), &observations)
            .unwrap()
            .approved
    );
    observations[1].success = true;
    observations[1].critical_regression = true;
    assert!(
        !evaluate_promotion(&context(), &observations)
            .unwrap()
            .approved
    );
}

#[test]
fn small_success_gains_do_not_round_up_to_ten_percentage_points() {
    let mut observations = trials(4, 2, 2);
    observations
        .iter_mut()
        .find(|t| t.variant == Variant::Candidate && !t.success)
        .unwrap()
        .success = true;
    let result = evaluate_promotion(&context(), &observations).unwrap();
    assert_eq!(
        result.metrics.candidate_successes - result.metrics.baseline_successes,
        1
    );
    assert!(!result.approved);
}

#[test]
fn novelty_needs_transfer_and_independent_ablation_evidence() {
    let observations = trials(2, 1, 2);
    assert!(
        !evaluate_novel_capability(&context(), &observations, &[])
            .unwrap()
            .approved
    );
    let mut ablated = snapshot("ablated");
    ablated.configuration_digest = "no-memory".into();
    let mut evidence = observations.clone();
    for trial in &mut evidence {
        if trial.variant == Variant::Baseline {
            trial.snapshot = ablated.clone();
        }
    }
    let ablation = AblationStudy {
        removed_component: "memory".into(),
        ablated_snapshot: ablated,
        observations: evidence,
    };
    assert!(
        evaluate_novel_capability(&context(), &observations, std::slice::from_ref(&ablation))
            .unwrap()
            .approved
    );
    let mut no_contribution = ablation;
    for trial in &mut no_contribution.observations {
        if trial.variant == Variant::Baseline {
            trial.success = true;
        }
    }
    assert!(
        !evaluate_novel_capability(&context(), &observations, &[no_contribution])
            .unwrap()
            .approved
    );
    assert!(
        !evaluate_novel_capability(&context(), &trials(2, 2, 3), &[])
            .unwrap()
            .approved
    );
}

fn record_window(budget: &mut ExplorationBudget, prefix: &str, promotions: usize, critical: bool) {
    for index in 0..10 {
        let result = budget
            .record(ExplorationOutcome {
                experiment_id: format!("{prefix}-{index}"),
                promoted_on_holdout: index < promotions,
                critical_regression: critical && index == 0,
            })
            .unwrap();
        assert_eq!(result.is_some(), index == 9);
    }
}

#[test]
fn exploration_uses_full_windows_survives_restart_and_respects_bounds() {
    let mut budget = ExplorationBudget::default();
    assert_eq!(budget.share_percent(), 25);
    record_window(&mut budget, "a", 2, false);
    assert_eq!(budget.share_percent(), 30);
    record_window(&mut budget, "b", 1, false);
    assert_eq!(budget.share_percent(), 30);
    record_window(&mut budget, "c", 10, true);
    assert_eq!(budget.share_percent(), 25);
    let mut restored: ExplorationBudget =
        serde_json::from_str(&serde_json::to_string(&budget).unwrap()).unwrap();
    assert!(
        restored
            .record(ExplorationOutcome {
                experiment_id: "a-0".into(),
                promoted_on_holdout: true,
                critical_regression: false
            })
            .is_err()
    );
    for index in 0..10 {
        record_window(&mut restored, &format!("down-{index}"), 0, false);
    }
    assert_eq!(restored.share_percent(), 10);
    for index in 0..10 {
        record_window(&mut restored, &format!("up-{index}"), 2, false);
    }
    assert_eq!(restored.share_percent(), 50);
}

#[test]
fn rule_changes_remain_quarantined_and_content_bound() {
    let proposal = QuarantinedRuleProposal::new(
        "current".into(),
        "threshold=0".into(),
        "isolated experiment".into(),
    )
    .unwrap();
    proposal.validate().unwrap();
    assert!(!proposal.can_adopt_automatically());
    let mut tampered = proposal;
    tampered.proposed_rule = "threshold=-1".into();
    assert!(tampered.validate().is_err());
}

#[test]
fn holdouts_cannot_overlap_optimizer_or_shared_knowledge() {
    let temp = tempfile::tempdir().unwrap();
    let private = temp.path().join("holdout");
    let optimizer = temp.path().join("optimizer");
    let knowledge = temp.path().join("superpod");
    for path in [&private, &optimizer, &knowledge] {
        std::fs::create_dir(path).unwrap();
    }
    let mut policy = HoldoutIsolation {
        private_root: private.clone(),
        optimizer_roots: vec![optimizer.clone()],
        shared_knowledge_roots: vec![knowledge],
        shared_memory_roots: vec![],
    };
    policy.validate().unwrap();
    policy.authorize_optimizer_path(&optimizer).unwrap();
    assert!(policy.authorize_optimizer_path(&private).is_err());
    policy
        .shared_knowledge_roots
        .push(temp.path().to_path_buf());
    assert!(policy.validate().is_err());
}

#[cfg(unix)]
#[test]
fn nested_symlinks_cannot_expose_holdout_answers() {
    let temp = tempfile::tempdir().unwrap();
    let private = temp.path().join("holdout");
    let optimizer = temp.path().join("optimizer");
    std::fs::create_dir(&private).unwrap();
    std::fs::create_dir(&optimizer).unwrap();
    std::os::unix::fs::symlink(&private, optimizer.join("answers")).unwrap();
    let policy = HoldoutIsolation {
        private_root: private,
        optimizer_roots: vec![optimizer],
        shared_knowledge_roots: vec![],
        shared_memory_roots: vec![],
    };
    assert!(policy.validate().is_err());
}
