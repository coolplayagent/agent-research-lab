use super::*;
use contracts::*;

async fn setup() -> (tempfile::TempDir, Hub, String) {
    let root = tempfile::tempdir().unwrap();
    let hub = Hub::open(root.path()).unwrap();
    hub.register_operator().await.unwrap();
    hub.register(vec!["a".into(), "b".into(), "outside".into()])
        .await
        .unwrap();
    hub.create_group(NewGroup {
        id: "team".into(),
        title: "Team".into(),
        topic: "Explore".into(),
        kind: GroupKind::Conversation,
        private: true,
        members: vec!["operator".into(), "a".into(), "b".into()],
    })
    .await
    .unwrap();
    hub.create_group(NewGroup {
        id: "other".into(),
        title: "Other".into(),
        topic: "Other".into(),
        kind: GroupKind::Conversation,
        private: true,
        members: vec!["operator".into(), "a".into()],
    })
    .await
    .unwrap();
    for goal in ["left", "right"] {
        hub.change_goal(GoalChange::Create {
            goal: GoalInput {
                id: goal.into(),
                group_id: "team".into(),
                title: goal.into(),
                objective: "Test a hypothesis".into(),
                acceptance: "Independent evidence".into(),
                scenario: "collaboration".into(),
                parameters: json!({}),
                assignments: vec![AssignmentInput {
                    person_id: if goal == "left" { "a" } else { "b" }.into(),
                    instruction: "Collect evidence".into(),
                }],
                max_seconds: 600,
            },
        })
        .await
        .unwrap();
    }
    let token = hub.grant("a", 600).await.unwrap();
    hub.work(
        &token,
        WorkChange::Claim {
            attempt: 1,
            goal_id: "left".into(),
            work_id: "work-0".into(),
            claim_id: "claim-a".into(),
        },
    )
    .await
    .unwrap();
    (root, hub, token)
}
fn evidence() -> Vec<OutcomeEvidence> {
    vec![OutcomeEvidence {
        repository: "fixture-repo".into(),
        commit: "a".repeat(40),
        path: "report.json".into(),
        sha256: "b".repeat(64),
    }]
}
fn input(id: &str) -> OutcomeInput {
    OutcomeInput {
        node_id: None,
        id: id.into(),
        group_id: "team".into(),
        goal_id: "left".into(),
        kind: OutcomeKind::Finding,
        title: "A reproducible finding".into(),
        summary: "The isolated experiment reproduced the failure".into(),
        conditions: "Only the pinned fixture and configuration".into(),
        evidence: evidence(),
        references: vec![],
        supersedes: None,
        attempt: Some(OutcomeAttempt {
            goal_id: "left".into(),
            work_id: "work-0".into(),
            attempt: 1,
            claim_id: "claim-a".into(),
        }),
        pins: EvidencePins {
            source: Some("a".repeat(40)),
            prompt: Some("c".repeat(64)),
            policy: Some("d".repeat(64)),
            superpod: None,
        },
        knowledge: None,
    }
}
async fn get(hub: &Hub, id: &str) -> OutcomeView {
    serde_json::from_value(
        hub.outcomes(None, OutcomeCommand::Get { id: id.into() })
            .await
            .unwrap(),
    )
    .unwrap()
}
async fn begin(hub: &Hub, id: &str, request: &str) -> OutcomeVerification {
    let entry = get(hub, id).await;
    let value = hub
        .outcomes(
            None,
            OutcomeCommand::BeginVerification {
                input: OutcomeVerificationInput {
                    id: request.into(),
                    outcome_id: id.into(),
                    outcome_digest: entry.outcome.digest,
                    verifier_id: "independent-verifier".into(),
                    verifier_digest: "e".repeat(64),
                    max_seconds: 60,
                },
            },
        )
        .await
        .unwrap();
    serde_json::from_value(value["record"].clone()).unwrap()
}
async fn passed(hub: &Hub, id: &str, request: &str) {
    let run = begin(hub, id, request).await;
    hub.outcomes(
        None,
        OutcomeCommand::FinishVerification {
            result: OutcomeVerificationResult {
                id: request.into(),
                request_digest: run.request_digest,
                state: VerificationRunState::Passed,
                summary: "Independent reproduction passed".into(),
                evidence: evidence(),
            },
        },
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn direct_publication_requires_current_authority_and_never_completes_a_goal() {
    let (_root, hub, token) = setup().await;
    let entry = input("one");
    let first = hub
        .outcomes(
            Some(&token),
            OutcomeCommand::Publish {
                input: entry.clone().into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(
        first["verification"],
        OutcomeVerificationState::Unverified.as_str()
    );
    assert_eq!(
        first["acceptance"],
        OutcomeAcceptanceState::Pending.as_str()
    );
    assert!(first["outcome"]["input"]["knowledge"].is_null());
    assert_eq!(
        hub.outcomes(
            Some(&token),
            OutcomeCommand::Publish {
                input: entry.clone().into()
            }
        )
        .await
        .unwrap(),
        first
    );
    let mut conflict = entry.clone();
    conflict.summary = "Different bytes".into();
    assert!(
        hub.outcomes(
            Some(&token),
            OutcomeCommand::Publish {
                input: conflict.into()
            }
        )
        .await
        .is_err()
    );
    let mut stale = input("stale");
    stale.attempt.as_mut().unwrap().attempt = 2;
    assert!(
        hub.outcomes(
            Some(&token),
            OutcomeCommand::Publish {
                input: stale.into()
            }
        )
        .await
        .is_err()
    );
    assert!(
        hub.outcomes(
            Some(&token),
            OutcomeCommand::Decide {
                decision: OutcomeDecision {
                    id: "forge".into(),
                    outcome_id: "one".into(),
                    verification_id: None,
                    state: OutcomeAcceptanceState::Accepted,
                    reason: "Self report".into()
                }
            }
        )
        .await
        .is_err()
    );
    let outsider = hub.grant("outside", 60).await.unwrap();
    for command in [
        OutcomeCommand::Get { id: "one".into() },
        OutcomeCommand::Search {
            query: OutcomeQuery {
                group_id: "team".into(),
                ..Default::default()
            },
        },
        OutcomeCommand::History {
            id: "one".into(),
            after: 0,
        },
    ] {
        assert!(hub.outcomes(Some(&outsider), command).await.is_err());
    }
    let goals = hub.goals("team", "", "", false).await.unwrap();
    assert_eq!(goals["goals"][0]["state"], GoalState::Active.as_str());
}

#[tokio::test]
async fn references_cross_paths_but_not_topics_and_survive_member_exit_and_restart() {
    let (root, hub, token) = setup().await;
    hub.outcomes(
        Some(&token),
        OutcomeCommand::Publish {
            input: input("one").into(),
        },
    )
    .await
    .unwrap();
    let token_b = hub.grant("b", 600).await.unwrap();
    hub.work(
        &token_b,
        WorkChange::Claim {
            attempt: 1,
            goal_id: "right".into(),
            work_id: "work-0".into(),
            claim_id: "claim-b".into(),
        },
    )
    .await
    .unwrap();
    let mut other = input("two");
    other.goal_id = "right".into();
    other.attempt = Some(OutcomeAttempt {
        goal_id: "right".into(),
        work_id: "work-0".into(),
        attempt: 1,
        claim_id: "claim-b".into(),
    });
    other.kind = OutcomeKind::Counterexample;
    other.references.push(OutcomeReference {
        outcome_id: "one".into(),
        kind: OutcomeReferenceKind::Challenges,
        reason: "Fails under a different condition".into(),
    });
    hub.outcomes(
        Some(&token_b),
        OutcomeCommand::Publish {
            input: other.clone().into(),
        },
    )
    .await
    .unwrap();
    other.id = "cross-topic".into();
    other.group_id = "other".into();
    assert!(
        hub.outcomes(
            Some(&token_b),
            OutcomeCommand::Publish {
                input: other.into()
            }
        )
        .await
        .is_err()
    );
    hub.work(
        &token,
        WorkChange::Submit {
            attempt: 1,
            goal_id: "left".into(),
            work_id: "work-0".into(),
            claim_id: "claim-a".into(),
            result: WorkResult {
                code: None,
                summary: "Publication finished".into(),
                succeeded: true,
                evidence: json!({}),
            },
        },
    )
    .await
    .unwrap();
    let goal: Goal = serde_json::from_value(
        hub.goals("team", "", "", false).await.unwrap()["goals"]
            .as_array()
            .unwrap()
            .iter()
            .find(|g| g["input"]["id"] == "left")
            .unwrap()
            .clone(),
    )
    .unwrap();
    hub.change_goal(GoalChange::Cancel {
        goal_id: "left".into(),
        revision: goal.revision,
    })
    .await
    .unwrap();
    hub.membership("team", "a", false).await.unwrap();
    assert!(
        hub.outcomes(Some(&token), OutcomeCommand::Get { id: "one".into() })
            .await
            .is_err()
    );
    assert!(
        hub.outcomes(Some(&token_b), OutcomeCommand::Get { id: "one".into() })
            .await
            .is_ok()
    );
    drop(hub);
    let reopened = Hub::open(root.path()).unwrap();
    assert_eq!(
        get(&reopened, "two").await.outcome.input.references[0].outcome_id,
        "one"
    );
    assert_eq!(
        get(&reopened, "one").await.publication,
        OutcomePublicationState::Published
    );
}

#[tokio::test]
async fn verification_receipts_are_fenced_and_acceptance_is_a_separate_record() {
    let (_root, hub, token) = setup().await;
    hub.outcomes(
        Some(&token),
        OutcomeCommand::Publish {
            input: input("one").into(),
        },
    )
    .await
    .unwrap();
    let decision = OutcomeDecision {
        id: "accept".into(),
        outcome_id: "one".into(),
        verification_id: Some("verify".into()),
        state: OutcomeAcceptanceState::Accepted,
        reason: "Meets the agreed criteria".into(),
    };
    assert!(
        hub.outcomes(
            None,
            OutcomeCommand::Decide {
                decision: decision.clone()
            }
        )
        .await
        .is_err()
    );
    let run = begin(&hub, "one", "verify").await;
    let mut result = OutcomeVerificationResult {
        id: "verify".into(),
        request_digest: "0".repeat(64),
        state: VerificationRunState::Passed,
        summary: "Checked".into(),
        evidence: evidence(),
    };
    assert!(
        hub.outcomes(
            None,
            OutcomeCommand::FinishVerification {
                result: result.clone()
            }
        )
        .await
        .is_err()
    );
    result.request_digest = run.request_digest;
    hub.outcomes(
        None,
        OutcomeCommand::FinishVerification {
            result: result.clone(),
        },
    )
    .await
    .unwrap();
    hub.outcomes(None, OutcomeCommand::FinishVerification { result })
        .await
        .unwrap();
    assert_eq!(
        get(&hub, "one").await.acceptance,
        OutcomeAcceptanceState::Pending
    );
    hub.outcomes(None, OutcomeCommand::Decide { decision })
        .await
        .unwrap();
    assert_eq!(
        get(&hub, "one").await.acceptance,
        OutcomeAcceptanceState::Accepted
    );
    let mut revision = input("revision");
    revision.supersedes = Some("one".into());
    hub.outcomes(
        Some(&token),
        OutcomeCommand::Publish {
            input: revision.into(),
        },
    )
    .await
    .unwrap();
    assert_eq!(
        get(&hub, "revision").await.verification,
        OutcomeVerificationState::Unverified
    );
    begin(&hub, "one", "recheck").await;
    assert_eq!(
        get(&hub, "one").await.acceptance,
        OutcomeAcceptanceState::Pending
    );
}

#[tokio::test]
async fn optional_knowledge_writes_require_acceptance_and_unknown_writes_are_reconciled() {
    let (_root, hub, token) = setup().await;
    hub.outcomes(
        Some(&token),
        OutcomeCommand::Publish {
            input: input("one").into(),
        },
    )
    .await
    .unwrap();
    let entry = get(&hub, "one").await;
    let write = KnowledgeWriteInput {
        id: "export".into(),
        outcome_id: "one".into(),
        outcome_digest: entry.outcome.digest,
        acceptance_id: "accept".into(),
        service_id: "optional-external-store".into(),
        service_digest: "f".repeat(64),
        repository: "knowledge-repo".into(),
        expected_revision: "a".repeat(40),
        max_seconds: 60,
    };
    assert!(
        hub.outcomes(
            None,
            OutcomeCommand::BeginWrite {
                input: write.clone()
            }
        )
        .await
        .is_err()
    );
    passed(&hub, "one", "verify").await;
    hub.outcomes(
        None,
        OutcomeCommand::Decide {
            decision: OutcomeDecision {
                id: "accept".into(),
                outcome_id: "one".into(),
                verification_id: Some("verify".into()),
                state: OutcomeAcceptanceState::Accepted,
                reason: "Reviewed independent receipt".into(),
            },
        },
    )
    .await
    .unwrap();
    let started = hub
        .outcomes(
            None,
            OutcomeCommand::BeginWrite {
                input: write.clone(),
            },
        )
        .await
        .unwrap();
    assert_eq!(started["created"], true);
    assert_eq!(
        hub.outcomes(
            None,
            OutcomeCommand::BeginWrite {
                input: write.clone()
            }
        )
        .await
        .unwrap()["created"],
        false
    );
    let run: KnowledgeWrite = serde_json::from_value(started["record"].clone()).unwrap();
    let mut result = KnowledgeWriteResult {
        id: "export".into(),
        request_digest: run.request_digest,
        state: KnowledgeWriteState::Unknown,
        reference: String::new(),
        revision: String::new(),
        summary: "Receipt lost".into(),
    };
    hub.outcomes(
        None,
        OutcomeCommand::FinishWrite {
            result: result.clone(),
        },
    )
    .await
    .unwrap();
    let mut repeat = write;
    repeat.id = "repeat".into();
    assert!(
        hub.outcomes(None, OutcomeCommand::BeginWrite { input: repeat })
            .await
            .is_err()
    );
    result.state = KnowledgeWriteState::Applied;
    result.reference = "knowledge/outcomes/one.md".into();
    result.revision = "b".repeat(40);
    result.summary = "Reconciled committed revision".into();
    hub.outcomes(None, OutcomeCommand::FinishWrite { result })
        .await
        .unwrap();
    let entry = get(&hub, "one").await;
    assert_eq!(
        entry.latest_write.unwrap().result.unwrap().state,
        KnowledgeWriteState::Applied
    );
    assert!(entry.outcome.input.knowledge.is_none());
    assert_eq!(
        hub.outcomes(
            None,
            OutcomeCommand::History {
                id: "one".into(),
                after: 0
            }
        )
        .await
        .unwrap()["events"]
            .as_array()
            .unwrap()
            .len(),
        6
    );
}

#[tokio::test]
async fn discovery_is_bounded_and_snapshot_pagination_keeps_new_publications_out() {
    let (_root, hub, token) = setup().await;
    for index in 0..35 {
        hub.outcomes(
            Some(&token),
            OutcomeCommand::Publish {
                input: input(&format!("entry-{index}")).into(),
            },
        )
        .await
        .unwrap();
    }
    let mut query = OutcomeQuery {
        group_id: "team".into(),
        query: "reproducible".into(),
        ..Default::default()
    };
    let page = hub
        .outcomes(
            Some(&token),
            OutcomeCommand::Search {
                query: query.clone(),
            },
        )
        .await
        .unwrap();
    assert_eq!(page["outcomes"].as_array().unwrap().len(), 32);
    query.snapshot = page["snapshot"].as_u64();
    query.after = page["next_after"].as_u64().unwrap();
    hub.outcomes(
        Some(&token),
        OutcomeCommand::Publish {
            input: input("later").into(),
        },
    )
    .await
    .unwrap();
    let next = hub
        .outcomes(Some(&token), OutcomeCommand::Search { query })
        .await
        .unwrap();
    assert_eq!(next["outcomes"].as_array().unwrap().len(), 3);
    assert!(next["next_after"].is_null());
    let mut invalid = input("oversize");
    invalid.summary = "x".repeat(4097);
    assert!(
        hub.outcomes(
            Some(&token),
            OutcomeCommand::Publish {
                input: invalid.into()
            }
        )
        .await
        .is_err()
    );
}
