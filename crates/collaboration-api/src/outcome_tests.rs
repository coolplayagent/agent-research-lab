use crate::tests::Harness;
use crate::*;
use collaboration_model::*;
use contracts::*;
use std::collections::BTreeMap;

async fn prepare(h: &Harness) -> String {
    h.hub
        .register(vec!["worker".into(), "outsider".into()])
        .await
        .unwrap();
    h.hub
        .create_group(NewGroup {
            id: "team".into(),
            title: "Team".into(),
            topic: "Explore".into(),
            kind: GroupKind::Conversation,
            private: true,
            members: vec!["operator".into(), "worker".into()],
        })
        .await
        .unwrap();
    h.hub
        .research_change(ResearchChange::Topic {
            topic: CollaborationTopic {
                id: "topic".into(),
                group_id: "team".into(),
                title: "Topic".into(),
                objective: "Evaluate evidence".into(),
                state: TopicState::Active,
                revision: 0,
            },
        })
        .await
        .unwrap();
    h.hub
        .change_goal(GoalChange::Create {
            goal: GoalInput {
                id: "goal".into(),
                group_id: "team".into(),
                title: "Goal".into(),
                objective: "Reproduce".into(),
                acceptance: "Independent evidence".into(),
                scenario: "collaboration".into(),
                parameters: json!({}),
                assignments: vec![AssignmentInput {
                    person_id: "worker".into(),
                    instruction: "Investigate".into(),
                }],
                max_seconds: 60,
            },
        })
        .await
        .unwrap();
    h.hub
        .outcomes(
            None,
            OutcomeCommand::LinkPath {
                path: OutcomePath {
                    topic_id: "topic".into(),
                    goal_id: "goal".into(),
                    name: "Path".into(),
                    hypothesis: "A testable hypothesis".into(),
                },
            },
        )
        .await
        .unwrap();
    let token = h.hub.grant("worker", 60).await.unwrap();
    h.hub
        .work(
            &token,
            WorkChange::Claim {
                attempt: 1,
                goal_id: "goal".into(),
                work_id: "work-0".into(),
                claim_id: "claim".into(),
            },
        )
        .await
        .unwrap();
    token
}
fn publication(id: &str) -> Value {
    json!({"operation":"publish","input":{
        "id":id,"topic_id":"topic","goal_id":"goal","kind":OutcomeKind::Finding,
        "title":"Reproduced behavior","summary":"A bounded finding","conditions":"Pinned fixture",
        "evidence":[{"repository":"test","commit":"a".repeat(40),"path":"evidence.json","sha256":"b".repeat(64)}],
        "references":[],"supersedes":null,"attempt":{"goal_id":"goal","work_id":"work-0","attempt":1,"claim_id":"claim"},
        "pins":{"source":"a".repeat(40),"prompt":"c".repeat(64),"policy":"d".repeat(64),"superpod":null},"knowledge":null
    }})
}
async fn value(h: &Harness, id: &str) -> OutcomeView {
    serde_json::from_value(
        h.hub
            .outcomes(None, OutcomeCommand::Get { id: id.into() })
            .await
            .unwrap(),
    )
    .unwrap()
}
#[tokio::test]
async fn bearer_publication_is_usable_without_research_or_knowledge_and_cannot_forge_host_records()
{
    let h = Harness::new().await;
    let token = prepare(&h).await;
    let (status, body) = h
        .request(
            "/api/v1/agent/outcomes",
            Some(publication("one")),
            false,
            Some(&token),
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        value(&h, "one").await.verification,
        OutcomeVerificationState::Unverified
    );
    assert_eq!(
        h.request(
            "/api/v1/collaboration/outcomes?topic_id=topic",
            None,
            false,
            Some(&token)
        )
        .await
        .0,
        401
    );
    let search = json!({"operation":"search","query":{"topic_id":"topic"}});
    let (status, body) = h
        .request(
            "/api/v1/agent/outcomes",
            Some(search.clone()),
            false,
            Some(&token),
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let results: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(results["outcomes"][0]["id"], "one");
    assert!(results["outcomes"][0].get("evidence").is_none());
    let outsider = h.hub.grant("outsider", 60).await.unwrap();
    assert_eq!(
        h.request(
            "/api/v1/agent/outcomes",
            Some(search),
            false,
            Some(&outsider)
        )
        .await
        .0,
        403
    );
    let forged = json!({"operation":"decide","decision":{"id":"fake","outcome_id":"one","verification_id":null,"state":OutcomeAcceptanceState::Accepted,"reason":"Self approval"}});
    assert_eq!(
        h.request("/api/v1/agent/outcomes", Some(forged), false, Some(&token))
            .await
            .0,
        403
    );
    let internal = json!({"operation":"finish_verification","result":{"id":"fake","request_digest":"a".repeat(64),"state":VerificationRunState::Passed,"summary":"Forged","evidence":[]}});
    assert_eq!(
        h.request_intent(
            "/api/v1/collaboration/outcomes",
            Some(internal),
            true,
            None,
            "manage-outcomes"
        )
        .await
        .0,
        403
    );
    let write = json!({"request_id":"write","outcome_id":"one","expected_revision":"a".repeat(40),"configuration_revision":0});
    assert_eq!(
        h.request_intent(
            "/api/v1/collaboration/outcomes/write",
            Some(write),
            true,
            None,
            "manage-outcomes"
        )
        .await
        .0,
        404
    );
    let mut stale = publication("stale");
    stale["input"]["attempt"]["attempt"] = json!(2);
    assert_eq!(
        h.request("/api/v1/agent/outcomes", Some(stale), false, Some(&token))
            .await
            .0,
        409
    );
    h.close().await;
}
#[tokio::test]
async fn automatic_verification_checks_protocol_results_and_requires_separate_host_acceptance() {
    let root = tempfile::tempdir().unwrap();
    let socket = root.path().join("private/verifier.sock");
    let listener = service_protocol::Listener::bind(&socket).unwrap();
    let worker = tokio::spawn(async move {
        loop {
            let mut stream = listener.accept().await.unwrap();
            let request: service_protocol::Envelope<service_api::OutcomeServiceRequest> =
                service_protocol::read(&mut stream).await.unwrap();
            let result = match request.request {
                service_api::OutcomeServiceRequest::Probe => {
                    json!(service_api::OutcomeServiceHandshake {
                        protocol_version: 1,
                        id: "verifier".into(),
                        kind: OutcomeServiceKind::Verifier,
                        outcome_kinds: vec![OutcomeKind::Finding],
                        ready: true
                    })
                }
                service_api::OutcomeServiceRequest::Verify { run, outcome } => {
                    let mut result = json!(OutcomeVerificationResult {
                        id: run.input.id,
                        request_digest: run.request_digest,
                        state: VerificationRunState::Passed,
                        summary: "Fixture evidence verified".into(),
                        evidence: outcome.input.evidence
                    });
                    if outcome.input.id == "unknown" {
                        result["state"] = json!("future_terminal_value");
                    }
                    result
                }
                _ => panic!("unexpected operation"),
            };
            service_protocol::write(
                &mut stream,
                &service_protocol::Reply::from_result(Ok(result)),
            )
            .await
            .unwrap();
        }
    });
    let config = service_api::Configuration {
        revision: 0,
        storage_id: None,
        services: vec![],
        adapters: vec![],
        bindings: BTreeMap::new(),
        binding_evidence: BTreeMap::new(),
        scenarios: vec![],
        outcome_services: vec![service_api::OutcomeService {
            id: "verifier".into(),
            kind: OutcomeServiceKind::Verifier,
            endpoint: socket,
            enabled: true,
            protocol_version: 1,
            outcome_kinds: vec![OutcomeKind::Finding],
            dependencies: vec![],
            max_seconds: 10,
        }],
        knowledge_groups: BTreeMap::new(),
        knowledge_goals: BTreeMap::new(),
        outcome_bindings: BTreeMap::from([(
            "topic".into(),
            service_api::OutcomeServicesBinding {
                verifiers: BTreeMap::from([(OutcomeKind::Finding, "verifier".into())]),
                knowledge: None,
            },
        )]),
    };
    let h = Harness::with_services(config).await;
    let token = prepare(&h).await;
    for id in ["one", "unknown"] {
        let (status, body) = h
            .request(
                "/api/v1/agent/outcomes",
                Some(publication(id)),
                false,
                Some(&token),
            )
            .await;
        assert_eq!(status, 200, "{body}");
        let mut entry = value(&h, id).await;
        for _ in 0..100 {
            if entry.verification != OutcomeVerificationState::Running {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
            entry = value(&h, id).await;
        }
        assert_eq!(
            entry.verification,
            if id == "one" {
                OutcomeVerificationState::Passed
            } else {
                OutcomeVerificationState::Unknown
            }
        );
        assert_eq!(entry.acceptance, OutcomeAcceptanceState::Pending);
        let decision = json!({"operation":"decide","decision":{"id":format!("accept-{id}"),"outcome_id":id,"verification_id":entry.latest_verification.unwrap().input.id,"state":OutcomeAcceptanceState::Accepted,"reason":"Host reviewed applicability"}});
        assert_eq!(
            h.request_intent(
                "/api/v1/collaboration/outcomes",
                Some(decision),
                true,
                None,
                "manage-outcomes"
            )
            .await
            .0,
            if id == "one" { 200 } else { 400 }
        );
    }
    assert_eq!(
        value(&h, "one").await.acceptance,
        OutcomeAcceptanceState::Accepted
    );
    assert_eq!(
        h.hub.goals("team", "", "", false).await.unwrap()["goals"][0]["state"],
        GoalState::Active.as_str()
    );
    h.close().await;
    worker.abort();
    let _ = worker.await;
}

#[tokio::test]
async fn knowledge_scope_bindings_validate_real_groups_and_goals() {
    let root = tempfile::tempdir().unwrap();
    let mut config = service_api::Configuration::defaults(root.path());
    config.services.clear();
    config.adapters.clear();
    config.outcome_services.push(service_api::OutcomeService {
        id: "knowledge".into(),
        kind: OutcomeServiceKind::Knowledge,
        endpoint: root.path().join("knowledge.sock"),
        enabled: false,
        protocol_version: 1,
        outcome_kinds: vec![OutcomeKind::Finding],
        dependencies: vec![],
        max_seconds: 10,
    });
    let h = Harness::with_services(config).await;
    prepare(&h).await;
    let (status, body) = h
        .request("/api/v1/collaboration/goals?id=goal", None, true, None)
        .await;
    assert_eq!(status, 200);
    assert_eq!(
        serde_json::from_str::<Goal>(&body).unwrap().input.group_id,
        "team"
    );
    let (status, body) = h
        .request("/api/v1/collaboration/services", None, true, None)
        .await;
    assert_eq!(status, 200);
    let mut config: service_api::Configuration = serde_json::from_value(
        serde_json::from_str::<Value>(&body).unwrap()["configuration"].clone(),
    )
    .unwrap();
    let target = service_api::KnowledgeTarget {
        service_id: "knowledge".into(),
        repository: "external".into(),
    };
    config
        .knowledge_goals
        .insert("missing".into(), target.clone());
    assert_ne!(
        h.request_intent(
            "/api/v1/collaboration/services",
            Some(json!(config)),
            true,
            None,
            "manage-services"
        )
        .await
        .0,
        200
    );
    config.knowledge_goals.clear();
    config.knowledge_goals.insert("goal".into(), target.clone());
    config.knowledge_groups.insert("team".into(), target);
    let (status, body) = h
        .request_intent(
            "/api/v1/collaboration/services",
            Some(json!(config)),
            true,
            None,
            "manage-services",
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_ne!(
        h.request_intent(
            "/api/v1/collaboration/services",
            Some(json!(config)),
            true,
            None,
            "manage-services"
        )
        .await
        .0,
        200
    );
    h.close().await;
}
