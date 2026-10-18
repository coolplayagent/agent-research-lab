use super::*;
use crate::{
    DatasetPartition, Variant,
    test_support::{context, snapshot, trials},
};

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
