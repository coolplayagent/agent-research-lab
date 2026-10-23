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

fn input() -> AgentInput {
    AgentInput {
        protocol_version: 1,
        request_id: "probe".into(),
        person_id: "coordinator".into(),
        goal_id: "task".into(),
        work_id: "plan".into(),
        attempt: 1,
        objective: "Plan".into(),
        acceptance: "A graph".into(),
        instruction: "Plan".into(),
        messages: vec![],
    }
}

#[test]
fn native_evidence_preserves_nested_proposals_and_rejects_invalid_content() {
    let input = input();
    let evidence = json!({"coordinator":{"protocol_version":1,"proposal":{"action":"clarify","question":"Scope?"}}});
    let mut native = json!({"protocol_version":1,"request_id":"probe","summary":"Ready","succeeded":true,"evidence_json":evidence.to_string()});
    assert_eq!(decode(&input, native.clone()).unwrap().evidence, evidence);
    for invalid in ["null", "[]", "invalid"] {
        native["evidence_json"] = json!(invalid);
        assert!(decode(&input, native.clone()).is_err());
    }
}

#[test]
fn native_output_keeps_exact_utf8_bounds_and_original_user_content() {
    let sample = include_str!("fixtures/utf8.txt").trim();
    let fill = |bytes: usize| {
        let value = sample.repeat(bytes / sample.len());
        let remainder = bytes - value.len();
        value + &"x".repeat(remainder)
    };
    let mut input = input();
    input.instruction = sample.into();
    let evidence = json!({"sources":fill(8192 - json!({"sources":""}).to_string().len())});
    let mut native = json!({"protocol_version":1,"request_id":"probe","summary":fill(8000),"succeeded":true,"evidence_json":evidence.to_string()});
    let output = decode(&input, native.clone()).unwrap();
    assert_eq!(output.summary.len(), 8000);
    assert_eq!(output.evidence.to_string().len(), 8192);
    assert_eq!(output.evidence, evidence);
    native["evidence_json"] = json!(evidence.to_string() + " ");
    let error = decode(&input, native.clone()).unwrap_err().to_string();
    assert!(error.contains("8193 UTF-8 bytes; maximum 8192"));
    native["evidence_json"] = json!(evidence.to_string());
    native["summary"] = json!(fill(8001));
    assert!(decode(&input, native).is_err());
    let prompt = prompt(&input).unwrap();
    assert!(prompt.contains("summary <= 8000 bytes"));
    assert!(prompt.contains("evidence_json string <= 8192 bytes"));
    assert!(prompt.ends_with(&serde_json::to_string(&input).unwrap()));
}
