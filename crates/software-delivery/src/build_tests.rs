use super::*;

#[test]
fn activation_rejects_empty_failed_or_stale_full_gate() {
    let commit = "a".repeat(40);
    let valid = json!({"profile":"full","scope":"delivery","gate":{"complete":true,"decision":"pass"},"selection":{"empty_delivery":false},"snapshot":{"mode":"diff","head":commit},"plan":{"pending_delivery_checks":[]}});
    validate_gate(&valid, &commit).unwrap();
    for (pointer, value) in [
        ("/selection/empty_delivery", json!(true)),
        ("/snapshot/head", json!("b".repeat(40))),
        ("/gate/complete", json!(false)),
        ("/plan/pending_delivery_checks", json!(["test"])),
    ] {
        let mut bad = valid.clone();
        *bad.pointer_mut(pointer).unwrap() = value;
        assert!(validate_gate(&bad, &commit).is_err());
    }
}

#[test]
fn merged_gate_rejects_missing_or_unknown_protocol_fields() {
    let commit = "a".repeat(40);
    let valid = json!({"profile":"full","scope":"task","gate":{"complete":true,"decision":"pass"},"selection":{"empty_delivery":false},"snapshot":{"mode":"diff","head":commit},"plan":{"pending_delivery_checks":[]}});
    validate_gate(&valid, &commit).unwrap();
    for (pointer, value) in [
        ("/profile", json!("quick")),
        ("/scope", json!("repository")),
        ("/scope", json!("future_scope")),
        ("/gate/decision", json!("fail")),
        ("/gate/decision", json!("unknown")),
        ("/gate/complete", json!("true")),
        ("/selection/empty_delivery", json!("false")),
        ("/snapshot/mode", json!("worktree")),
        ("/snapshot/head", Value::Null),
        ("/plan/pending_delivery_checks", Value::Null),
    ] {
        let mut bad = valid.clone();
        *bad.pointer_mut(pointer).unwrap() = value;
        assert!(validate_gate(&bad, &commit).is_err(), "accepted {pointer}");
    }
    for key in ["profile", "scope", "gate", "selection", "snapshot", "plan"] {
        let mut bad = valid.clone();
        bad.as_object_mut().unwrap().remove(key);
        assert!(
            validate_gate(&bad, &commit).is_err(),
            "accepted missing {key}"
        );
    }
}
