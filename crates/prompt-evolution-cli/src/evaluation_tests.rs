use crate::{
    EvaluationReceipt, execute,
    files::read_strict,
    test_support::{example, prepare_gate, setup},
};
use serde_json::Value;

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
fn request_order_does_not_change_receipt_and_outputs_cannot_replace_inputs() {
    let temp = setup();
    let input = prepare_gate(temp.path(), "evaluate");
    let output = temp.path().join("host/receipt.json");
    execute("evaluate", &input, Some(&output)).unwrap();
    let first = std::fs::read(&output).unwrap();
    let mut request: Value = storage::read(&input).unwrap();
    request["observations"].as_array_mut().unwrap().reverse();
    storage::write(&input, &request).unwrap();
    execute("evaluate", &input, Some(&output)).unwrap();
    assert_eq!(first, std::fs::read(&output).unwrap());
    for protected in [&input, &temp.path().join("host/context.json")] {
        let before = std::fs::read(protected).unwrap();
        assert!(execute("evaluate", &input, Some(protected)).is_err());
        assert_eq!(before, std::fs::read(protected).unwrap());
    }
}
