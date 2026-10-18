use super::*;

#[test]
fn merge_receipts_require_explicit_merged_state_and_preserve_observations() {
    for (wire, expected) in [
        (
            PullRequestState::Open.as_str(),
            QueueState::NeedsReconciliation,
        ),
        (
            PullRequestState::Closed.as_str(),
            QueueState::NeedsReconciliation,
        ),
        (PullRequestState::Merged.as_str(), QueueState::Complete),
        ("future_state", QueueState::NeedsReconciliation),
        ("", QueueState::NeedsReconciliation),
    ] {
        let receipt = delivery::PullRequestReceipt {
            number: 1,
            url: "https://github.com/example/project/pull/1".into(),
            state: wire.into(),
            head: "a".repeat(40),
            reused: true,
        };
        let raw = serde_json::to_value(&receipt).unwrap();
        let outcome = Outcome::merge(receipt).unwrap();
        assert_eq!(outcome.state, expected);
        assert_eq!(outcome.result, raw);
        assert_eq!(
            outcome.error.is_some(),
            expected == QueueState::NeedsReconciliation
        );
    }
}

#[test]
fn refresh_uses_typed_readiness_and_retains_full_raw_status() {
    for ready in [false, true] {
        let receipt = knowledge::IndexRefreshStatus {
            expected_commit: "a".repeat(40),
            ready,
            status: serde_json::json!({"adapter_detail":{"count":3},"state":"complete"}),
        };
        let raw = serde_json::to_value(&receipt).unwrap();
        let outcome = Outcome::refresh(receipt).unwrap();
        assert_eq!(
            outcome.state,
            if ready {
                QueueState::Complete
            } else {
                QueueState::Waiting
            }
        );
        assert_eq!(outcome.result, raw);
        assert!(outcome.error.is_none());
    }
}
