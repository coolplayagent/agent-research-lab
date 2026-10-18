use super::*;
use serde_json::json;

#[test]
fn observations_accept_canonical_and_historical_failure_states_without_guessing() {
    for state in WorkflowState::ALL {
        let wire = serde_json::to_value(state).unwrap();
        assert_eq!(parse_run_state(&wire), Some(*state));
        assert_eq!(wire, state.as_str());
    }
    assert_eq!(
        parse_run_state(&json!("canceled")),
        Some(WorkflowState::Cancelled)
    );
    assert_eq!(
        parse_run_state(&json!("timed_out")),
        Some(WorkflowState::Failed)
    );
    for unknown in [
        Value::Null,
        json!(true),
        json!(1),
        json!({"status":"succeeded"}),
        json!(["succeeded"]),
        json!("Succeeded"),
        json!("succeeded "),
        json!("future_terminal"),
        json!(""),
    ] {
        assert_eq!(parse_run_state(&unknown), None);
    }
}
