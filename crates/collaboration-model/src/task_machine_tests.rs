use super::*;
fn members() -> BTreeSet<String> {
    ["alice".into(), "bob".into()].into()
}
fn node(id: &str, person: &str, dependencies: &[&str]) -> TaskNodeInput {
    TaskNodeInput {
        id: id.into(),
        person_id: person.into(),
        title: id.into(),
        instruction: "Perform the assignment".into(),
        deliverable: "A result".into(),
        acceptance: "Supporting explanation".into(),
        dependencies: dependencies.iter().map(|s| s.to_string()).collect(),
    }
}
fn task() -> OutcomeTask {
    OutcomeTask::new(
        TaskInput {
            id: "test".into(),
            group_id: "team".into(),
            intent: "Produce a comparison".into(),
            budget: TaskBudget::default(),
        },
        1000,
    )
    .unwrap()
}
#[test]
fn graph_rejects_cycles_and_outsiders_and_allows_repeated_person() {
    assert!(
        validate_graph(
            &[node("a", "alice", &["b"]), node("b", "alice", &["a"])],
            &members()
        )
        .is_err()
    );
    assert!(validate_graph(&[node("a", "outsider", &[])], &members()).is_err());
    assert!(
        validate_graph(
            &[node("a", "alice", &[]), node("b", "alice", &["a"])],
            &members()
        )
        .is_ok()
    );
}
#[test]
fn dependencies_need_checked_artifacts_and_stale_submissions_are_rejected() {
    let mut t = task();
    t.replace_graph(
        vec![node("a", "alice", &[]), node("b", "bob", &["a"])],
        &members(),
        1000,
    )
    .unwrap();
    assert!(t.ready(&t.nodes[0]));
    assert!(!t.ready(&t.nodes[1]));
    t.transition(
        TaskAction::Claim {
            node_id: Some("a".into()),
            claim_id: "run".into(),
        },
        &members(),
        1001,
    )
    .unwrap();
    t.transition(
        TaskAction::Submit {
            node_id: "a".into(),
            claim_id: "run".into(),
            summary: "Actual source finding".into(),
            evidence: serde_json::json!({}),
            receipt: serde_json::json!({}),
            succeeded: true,
        },
        &members(),
        1002,
    )
    .unwrap();
    assert!(!t.ready(&t.nodes[1]));
    t.transition(
        TaskAction::Claim {
            node_id: None,
            claim_id: "check".into(),
        },
        &members(),
        1003,
    )
    .unwrap();
    t.transition(
        TaskAction::Coordinate {
            claim_id: "check".into(),
            output: CoordinatorOutput {
                protocol_version: 1,
                proposal: CoordinatorProposal::Check {
                    reviews: vec![NodeReview {
                        node_id: "a".into(),
                        approved: true,
                        reason: "The requested finding is present".into(),
                    }],
                },
            },
            receipt: serde_json::json!({}),
        },
        &members(),
        1004,
    )
    .unwrap();
    assert!(t.ready(&t.nodes[1]));
    t.transition(
        TaskAction::Claim {
            node_id: Some("b".into()),
            claim_id: "downstream".into(),
        },
        &members(),
        1005,
    )
    .unwrap();
    t.transition(TaskAction::Cancel, &members(), 1006).unwrap();
    assert!(
        t.transition(
            TaskAction::Submit {
                node_id: "b".into(),
                claim_id: "downstream".into(),
                summary: "late".into(),
                evidence: serde_json::json!({}),
                receipt: serde_json::json!({}),
                succeeded: true
            },
            &members(),
            1007
        )
        .is_err()
    );
}
#[test]
fn budget_clock_pauses_and_does_not_count_human_wait() {
    let mut t = task();
    t.state = TaskState::WaitingInput;
    t.advance_clock(999999);
    assert_eq!(t.active_ms, 0);
    t.transition(
        TaskAction::Supplement {
            text: "Proceed".into(),
        },
        &members(),
        1000000,
    )
    .unwrap();
    t.advance_clock(3000000);
    assert_eq!(t.state, TaskState::Paused);
    assert_eq!(t.error, Some(TaskErrorCode::Budget));
}
#[test]
fn replacing_upstream_invalidates_descendants() {
    let mut t = task();
    let original = vec![node("a", "alice", &[]), node("b", "bob", &["a"])];
    t.replace_graph(original.clone(), &members(), 1000).unwrap();
    for n in &mut t.nodes {
        n.state = TaskNodeState::Approved;
        n.artifact_id = Some(format!("old-{}", n.input.id));
        n.attempt = 1;
    }
    let mut revised = original;
    revised[0].instruction = "New scope".into();
    t.replace_graph(revised, &members(), 1001).unwrap();
    assert!(
        t.nodes
            .iter()
            .all(|n| n.state == TaskNodeState::Pending && n.artifact_id.is_none())
    );
}

#[test]
fn removing_and_readding_a_node_cannot_reset_its_attempt_budget() {
    let mut t = task();
    t.replace_graph(vec![node("a", "alice", &[])], &members(), 1000)
        .unwrap();
    t.nodes[0].attempt = 3;
    t.nodes[0].state = TaskNodeState::Failed;
    t.replace_graph(vec![node("b", "bob", &[])], &members(), 1001)
        .unwrap();
    assert!(
        t.replace_graph(vec![node("a", "alice", &[])], &members(), 1002)
            .is_err()
    );
    assert_eq!(t.retired_nodes[0].attempt, 3);
}
#[test]
fn a_returned_delivery_cannot_be_resubmitted_without_a_new_graph() {
    let mut t = task();
    t.replace_graph(vec![node("a", "alice", &[])], &members(), 1000)
        .unwrap();
    t.nodes[0].state = TaskNodeState::Approved;
    t.nodes[0].artifact_id = Some("output".into());
    t.apply_proposal(
        CoordinatorProposal::Deliver {
            summary: "Deliverable".into(),
            node_ids: vec!["a".into()],
        },
        &members(),
        1001,
    )
    .unwrap();
    t.transition(
        TaskAction::Return {
            reason: "Address the missing comparison".into(),
        },
        &members(),
        1002,
    )
    .unwrap();
    assert!(
        t.apply_proposal(
            CoordinatorProposal::Deliver {
                summary: "Same deliverable".into(),
                node_ids: vec!["a".into()]
            },
            &members(),
            1003
        )
        .is_err()
    );
    assert_eq!(t.deliveries.len(), 1);
    assert!(t.deliveries[0].accepted_ms.is_none());
}

#[test]
fn failed_questions_pause_and_retry_with_a_fresh_attempt() {
    let mut t = task();
    t.kind = TaskKind::Question;
    t.replace_graph(vec![node("a", "alice", &[])], &members(), 1000)
        .unwrap();
    t.transition(
        TaskAction::Claim {
            node_id: Some("a".into()),
            claim_id: "one".into(),
        },
        &members(),
        1001,
    )
    .unwrap();
    t.transition(
        TaskAction::Fail {
            node_id: Some("a".into()),
            claim_id: "one".into(),
            code: TaskErrorCode::Unavailable,
        },
        &members(),
        1002,
    )
    .unwrap();
    assert_eq!(t.state, TaskState::Paused);
    t.transition(TaskAction::Resume, &members(), 1003).unwrap();
    assert!(t.ready(&t.nodes[0]));
    t.transition(
        TaskAction::Claim {
            node_id: Some("a".into()),
            claim_id: "two".into(),
        },
        &members(),
        1004,
    )
    .unwrap();
    assert_eq!(t.nodes[0].attempt, 2);
    t.transition(
        TaskAction::Submit {
            node_id: "a".into(),
            claim_id: "two".into(),
            summary: "An answer".into(),
            evidence: serde_json::json!({}),
            receipt: serde_json::json!({}),
            succeeded: true,
        },
        &members(),
        1005,
    )
    .unwrap();
    assert_eq!(t.state, TaskState::Completed);
    assert!(t.deliveries.is_empty());
}

#[test]
fn transient_coordinator_failures_retry_within_the_attempt_bound() {
    for code in [TaskErrorCode::Unavailable, TaskErrorCode::Protocol] {
        let mut t = task();
        for attempt in 1..=3 {
            t.transition(
                TaskAction::Claim {
                    node_id: None,
                    claim_id: format!("claim-{attempt}"),
                },
                &members(),
                1000 + attempt * 2,
            )
            .unwrap();
            t.transition(
                TaskAction::Fail {
                    node_id: None,
                    claim_id: format!("claim-{attempt}"),
                    code,
                },
                &members(),
                1001 + attempt * 2,
            )
            .unwrap();
            assert_eq!(t.needs_coordinator(), attempt < 3);
        }
        assert_eq!(t.state, TaskState::Paused);
        assert_eq!(t.error, Some(code));
        assert!(
            t.coordinator_attempts
                .iter()
                .all(|a| a.finished_ms.is_some())
        );
    }
}
