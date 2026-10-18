use crate::{
    PromotionRequest, execute,
    files::read_strict,
    test_support::{example, execute_fixture, prepare_gate, setup},
};
use evolution::ExplorationBudget;
use serde_json::json;
use std::fs;

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
fn later_invalid_receipt_cannot_create_or_partially_update_budget() {
    let temp = setup();
    let gate = prepare_gate(temp.path(), "evaluate");
    let receipt = temp.path().join("host/evaluation-receipt.json");
    execute("evaluate", &gate, Some(&receipt)).unwrap();
    let mut corrupt: serde_json::Value = storage::read(&receipt).unwrap();
    corrupt["decision"]["approved"] = false.into();
    storage::write(&temp.path().join("host/corrupt-receipt.json"), &corrupt).unwrap();
    let mut request = example("exploration");
    request["experiments"] = json!([
        {"experiment_id":"valid-first","evaluation_receipt":"evaluation-receipt.json"},
        {"experiment_id":"invalid-second","evaluation_receipt":"corrupt-receipt.json"}
    ]);
    let input = temp.path().join("host/exploration.json");
    let output = temp.path().join("host/budget.json");
    storage::write(&input, &request).unwrap();
    let error = execute_fixture("exploration", &input, Some(&output)).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("does not match recomputed evidence")
    );
    assert!(!output.exists());
    storage::write(&output, &ExplorationBudget::default()).unwrap();
    let before = fs::read(&output).unwrap();
    let error = execute_fixture("exploration", &input, Some(&output)).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("does not match recomputed evidence")
    );
    assert_eq!(before, fs::read(&output).unwrap());
}
