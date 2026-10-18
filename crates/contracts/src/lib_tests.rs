use super::*;
#[test]
fn wire_values_roundtrip_and_reject_unknown_states() {
    for state in ExecutionState::ALL {
        assert_eq!(ExecutionState::parse(state.as_str()).unwrap(), *state);
        assert_eq!(serde_json::to_value(state).unwrap(), state.as_str());
    }
    assert!(WorkflowState::from_value(&serde_json::json!("future_state")).is_none());
    assert!(GoalState::parse("succeeded").is_err());
    assert_eq!(Presence::parse("busy").unwrap(), Presence::Busy);
    assert_eq!(
        serde_json::to_value(OriginState::Completed {
            receipt_sha256: "abc".into()
        })
        .unwrap(),
        serde_json::json!({"state":"completed","receipt_sha256":"abc"})
    );
}
