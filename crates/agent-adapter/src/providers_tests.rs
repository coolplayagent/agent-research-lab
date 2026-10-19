use super::*;

#[test]
fn launcher_flags_are_forwarded_as_values_without_consuming_bridge_options() {
    use clap::Parser;
    let options = Options::try_parse_from([
        "bridge",
        "--agent",
        "codex",
        "--program",
        "node.exe",
        "--digest",
        "fixture",
        "--launcher-argument",
        "--preserve-symlinks-main",
        "--launcher-argument",
        "--preserve-symlinks",
        "--launcher-argument",
        "cli.js",
        "--model",
        "fixture-model",
    ])
    .unwrap();
    assert_eq!(
        options.launcher_argument,
        ["--preserve-symlinks-main", "--preserve-symlinks", "cli.js"]
    );
    assert_eq!(options.model.as_deref(), Some("fixture-model"));
}
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
