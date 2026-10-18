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

#[test]
fn installation_journal_values_remain_distinct_from_service_activation() {
    let legacy = [
        "staged",
        "previous_backed_up",
        "activated",
        "verified",
        "rolled_back",
    ];
    assert_eq!(InstallationPhase::ALL.len(), legacy.len());
    for wire in legacy {
        let phase: InstallationPhase = serde_json::from_value(serde_json::json!(wire)).unwrap();
        assert_eq!(serde_json::to_value(phase).unwrap(), wire);
    }
    assert!(InstallationPhase::parse("active").is_err());
    assert!(InstallationPhase::parse("future_phase").is_err());
    assert!(ActivationPhase::parse("activated").is_err());
}

#[test]
fn publication_domains_preserve_their_distinct_wire_values() {
    assert_eq!(
        SourceAccessStatus::ALL
            .iter()
            .map(|value| value.as_str())
            .collect::<Vec<_>>(),
        [
            "downloaded-verified",
            "online-only",
            "access-gated",
            "not-found",
            "candidate"
        ]
    );
    assert_eq!(
        FindingKind::ALL
            .iter()
            .map(|value| value.as_str())
            .collect::<Vec<_>>(),
        ["measured", "external_claim", "inference", "unknown"]
    );
    assert!(SourceAccessStatus::parse("unknown").is_err());
    assert_eq!(FindingKind::parse("unknown").unwrap(), FindingKind::Unknown);
    assert!(ContentIntegrityState::parse("downloaded-verified").is_err());
}
