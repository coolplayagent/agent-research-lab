use super::*;
use serde_json::{Value, json};

#[test]
fn legacy_report_wire_and_unicode_markdown_remain_stable() {
    let legacy: Value = serde_json::from_str(include_str!("../fixtures/report-wire.json")).unwrap();
    let report: OutcomeReport = serde_json::from_value(legacy.clone()).unwrap();
    assert_eq!(serde_json::to_value(&report).unwrap(), legacy);
    assert_eq!(
        serde_json::to_string(&report).unwrap(),
        include_str!("../fixtures/report-wire.json").trim_end()
    );
    assert_eq!(
        render_report(&report),
        include_str!("../fixtures/report-rendered.md")
    );
    assert_eq!(
        report_digest(&report),
        "b49aa31f36f4755896856b4048aefdd712ea51fb04cb466dcfb01431483e3c51"
    );
    let mut alias = legacy.clone();
    alias["bindings"]["knowledge_revision"] = alias["bindings"]
        .as_object_mut()
        .unwrap()
        .remove("superpod_commit")
        .unwrap();
    let aliased: OutcomeReport = serde_json::from_value(alias).unwrap();
    assert_eq!(serde_json::to_value(aliased).unwrap(), legacy);
}

#[test]
fn source_catalog_rejects_unknown_or_malformed_access_states_on_read() {
    let legacy: Value = serde_json::from_str(include_str!("../fixtures/report-wire.json")).unwrap();
    for state in [
        json!("future_status"),
        json!("complete"),
        json!("unknown"),
        Value::Null,
        json!(true),
        json!(1),
    ] {
        let mut input = legacy.clone();
        input["sources"][0]["access_status"] = state;
        assert!(serde_json::from_value::<OutcomeReport>(input).is_err());
    }
    let mut missing = legacy;
    missing["sources"][0]
        .as_object_mut()
        .unwrap()
        .remove("access_status");
    assert!(serde_json::from_value::<OutcomeReport>(missing).is_err());
}
