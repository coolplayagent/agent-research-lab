use super::*;
use std::collections::BTreeMap;

#[test]
fn rejected_admission_does_not_leave_a_phantom_pending_activation() {
    let mut round = collaboration_model::EvolutionRound {
        number: 1,
        state: RoundState::Accepted,
        goal_id: "goal".into(),
        goal_attempts: BTreeMap::new(),
        baseline_version_id: "baseline".into(),
        started_ms: 1,
        finished_ms: None,
        feedback: String::new(),
        candidate: None,
        evaluation: None,
        activation: None,
    };
    assert_eq!(confirmed_round_claim(&round, "claim", false).unwrap(), None);
    assert!(confirmed_round_claim(&round, "claim", true).is_err());
    round.activation = Some(collaboration_model::CandidateActivation {
        state: ActivationState::Pending,
        reference: "claim".into(),
        evidence: json!({"activation_claim_id":"claim"}),
    });
    assert_eq!(
        confirmed_round_claim(&round, "claim", false).unwrap(),
        Some("claim".into())
    );
    assert!(confirmed_round_claim(&round, "different-claim", false).is_err());
}

#[test]
fn switched_operations_require_the_matching_pending_durable_claim() {
    let mut round = collaboration_model::EvolutionRound {
        number: 1,
        state: RoundState::Accepted,
        goal_id: "goal".into(),
        goal_attempts: BTreeMap::new(),
        baseline_version_id: "baseline".into(),
        started_ms: 1,
        finished_ms: None,
        feedback: String::new(),
        candidate: None,
        evaluation: None,
        activation: None,
    };
    assert!(confirmed_round_claim(&round, "claim", true).is_err());
    for state in [
        ActivationState::Pending,
        ActivationState::Active,
        ActivationState::Failed,
        ActivationState::RolledBack,
    ] {
        round.activation = Some(collaboration_model::CandidateActivation {
            state,
            reference: "claim".into(),
            evidence: json!({"activation_claim_id":"claim"}),
        });
        for switched in [false, true] {
            if state == ActivationState::Pending {
                assert_eq!(
                    confirmed_round_claim(&round, "claim", switched).unwrap(),
                    Some("claim".into())
                );
            } else {
                assert!(confirmed_round_claim(&round, "claim", switched).is_err());
            }
            assert!(confirmed_round_claim(&round, "different-claim", switched).is_err());
        }
    }
}

#[test]
fn drain_release_only_removes_its_own_readable_operation_marker() {
    let dir = tempfile::tempdir().unwrap();
    let prepared = crate::tests::prepared(dir.path());
    let operation = dir.path().join("operation");
    let marker = dir.path().join("private/activation-drain");
    release_drain(&operation, &prepared).unwrap();
    storage::write(&marker, &json!({"operation":"another-operation"})).unwrap();
    let other = fs::read(&marker).unwrap();
    release_drain(&operation, &prepared).unwrap();
    assert_eq!(fs::read(&marker).unwrap(), other);
    fs::write(&marker, b"{truncated").unwrap();
    assert!(release_drain(&operation, &prepared).is_err());
    assert_eq!(fs::read(&marker).unwrap(), b"{truncated");
    storage::write(&marker, &json!({"operation":operation})).unwrap();
    release_drain(&operation, &prepared).unwrap();
    assert!(!marker.exists());
}
