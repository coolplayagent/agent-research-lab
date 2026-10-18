use crate::{
    EvaluationReceipt, execute,
    files::read_strict,
    test_support::{prepare_gate, setup},
};
use serde_json::Value;

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

#[test]
fn receipt_rejects_noncanonical_order_and_foreign_upstream_binding() {
    let temp = setup();
    let input = prepare_gate(temp.path(), "novelty");
    let output = temp.path().join("host/receipt.json");
    execute("novelty", &input, Some(&output)).unwrap();
    let valid: EvaluationReceipt = read_strict(&output).unwrap();
    let mut changed = valid.clone();
    changed.observations.reverse();
    assert!(
        changed
            .validate()
            .unwrap_err()
            .to_string()
            .contains("canonical order")
    );
    let mut changed = valid.clone();
    changed.ablations[0].observations.reverse();
    assert!(
        changed
            .validate()
            .unwrap_err()
            .to_string()
            .contains("canonical order")
    );
    let mut changed = valid.clone();
    changed.upstream.as_mut().unwrap().repository = "coolplayagent/different".into();
    assert!(
        changed
            .validate()
            .unwrap_err()
            .to_string()
            .contains("recomputed evidence")
    );
    let mut changed = valid.clone();
    changed.upstream.as_mut().unwrap().commit = "e".repeat(40);
    assert!(changed.validate().is_err());
    let mut changed = valid.clone();
    changed.skills_manifest = None;
    assert!(changed.validate().is_err());
    valid.validate().unwrap();
}
