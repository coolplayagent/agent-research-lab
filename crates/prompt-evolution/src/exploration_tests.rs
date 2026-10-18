use super::*;

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
fn rejected_exploration_replay_preserves_all_retained_budget_bytes() {
    let mut budget = ExplorationBudget::default();
    let outcome = ExplorationOutcome {
        experiment_id: "once".into(),
        promoted_on_holdout: true,
        critical_regression: false,
    };
    budget.record(outcome.clone()).unwrap();
    let before = serde_json::to_vec(&budget).unwrap();
    assert!(budget.record(outcome.clone()).is_err());
    assert_eq!(serde_json::to_vec(&budget).unwrap(), before);
    budget.completed_experiments.clear();
    let invalid = serde_json::to_vec(&budget).unwrap();
    assert!(
        budget
            .record(ExplorationOutcome {
                experiment_id: "later".into(),
                ..outcome
            })
            .is_err()
    );
    assert_eq!(serde_json::to_vec(&budget).unwrap(), invalid);
}
