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

#[test]
fn research_evaluation_domains_preserve_distinct_wire_values() {
    for (wire, state) in [
        ("development", DatasetPartition::Development),
        ("validation", DatasetPartition::Validation),
        ("holdout", DatasetPartition::Holdout),
    ] {
        assert_eq!(DatasetPartition::parse(wire).unwrap(), state);
        assert_eq!(
            serde_json::to_string(&state).unwrap(),
            format!("\"{wire}\"")
        );
    }
    for (wire, state) in [
        ("baseline", TrialVariant::Baseline),
        ("candidate", TrialVariant::Candidate),
    ] {
        assert_eq!(TrialVariant::parse(wire).unwrap(), state);
        assert_eq!(
            serde_json::to_string(&state).unwrap(),
            format!("\"{wire}\"")
        );
    }
    for (wire, state) in [
        ("promotion", PromptEvaluationKind::Promotion),
        ("novelty", PromptEvaluationKind::Novelty),
    ] {
        assert_eq!(PromptEvaluationKind::parse(wire).unwrap(), state);
        assert_eq!(
            serde_json::to_string(&state).unwrap(),
            format!("\"{wire}\"")
        );
    }
    assert!(DatasetPartition::parse("candidate").is_err());
    assert!(TrialVariant::parse("holdout").is_err());
    assert!(PromptEvaluationKind::parse("approved").is_err());
    for value in [
        serde_json::json!("future_state"),
        serde_json::Value::Null,
        serde_json::json!(true),
    ] {
        assert!(DatasetPartition::from_value(&value).is_none());
        assert!(TrialVariant::from_value(&value).is_none());
        assert!(PromptEvaluationKind::from_value(&value).is_none());
    }
}

#[test]
fn external_qualitygate_domains_keep_wire_values_and_fail_on_unknown_inputs() {
    for (name, values) in [
        ("QualitygateProfile", vec!["quick", "full"]),
        (
            "QualitygateScope",
            vec!["repository", "delivery", "task", "path"],
        ),
        ("QualitygateDecision", vec!["pass", "fail", "incomplete"]),
        (
            "QualitygateSnapshotMode",
            vec!["diff", "staged", "worktree", "mr"],
        ),
    ] {
        assert_eq!(catalog()[name], serde_json::json!(values));
    }
    assert_eq!(
        QualitygateDecision::parse("incomplete").unwrap(),
        QualitygateDecision::Incomplete
    );
    assert!(VerificationVerdict::parse("incomplete").is_err());
    for value in [
        serde_json::json!("future"),
        serde_json::json!(true),
        serde_json::Value::Null,
    ] {
        assert!(QualitygateProfile::from_value(&value).is_none());
        assert!(QualitygateScope::from_value(&value).is_none());
        assert!(QualitygateDecision::from_value(&value).is_none());
        assert!(QualitygateSnapshotMode::from_value(&value).is_none());
    }
}
