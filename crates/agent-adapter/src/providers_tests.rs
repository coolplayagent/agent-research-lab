use super::*;
#[test]
fn claude_unknown_and_failed_results_cannot_be_promoted() {
    for value in [
        json!({"type":"result","is_error":true,"subtype":"success","structured_output":{}}),
        json!({"type":"result","is_error":false,"subtype":"future_terminal","structured_output":{}}),
        json!({"type":"result","is_error":false,"subtype":"success"}),
    ] {
        assert!(validate_claude(&value).is_err());
    }
    assert!(
        validate_claude(
            &json!({"type":"result","is_error":false,"subtype":"success","structured_output":{}})
        )
        .is_ok()
    );
}
#[test]
fn external_unknown_completion_reasons_are_rejected() {
    assert!(contracts::PiStopReason::parse("future_success").is_err());
    assert!(contracts::DshTurnReason::parse("idle").is_err());
    assert!(CodexEventKind::parse("turn.maybe_completed").is_err());
}
