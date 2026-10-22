use crate::tests::Harness;
use crate::*;
use collaboration_model::*;
use contracts::*;

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
        "id":id,"group_id":"team","goal_id":"goal","kind":OutcomeKind::Finding,
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
            "/api/v2/agent/outcomes",
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
            "/api/v2/collaboration/outcomes?group_id=topic",
            None,
            false,
            Some(&token)
        )
        .await
        .0,
        401
    );
    let search = json!({"operation":"search","query":{"group_id":"team"}});
    let (status, body) = h
        .request(
            "/api/v2/agent/outcomes",
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
            "/api/v2/agent/outcomes",
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
        h.request("/api/v2/agent/outcomes", Some(forged), false, Some(&token))
            .await
            .0,
        403
    );
    let internal = json!({"operation":"finish_verification","result":{"id":"fake","request_digest":"a".repeat(64),"state":VerificationRunState::Passed,"summary":"Forged","evidence":[]}});
    assert_eq!(
        h.request_intent(
            "/api/v2/collaboration/outcomes",
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
            "/api/v2/collaboration/outcomes/write",
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
        h.request("/api/v2/agent/outcomes", Some(stale), false, Some(&token))
            .await
            .0,
        409
    );
    h.close().await;
}
#[tokio::test]
async fn host_verification_reuses_pinned_evidence_and_requires_separate_acceptance() {
    let h = Harness::new().await;
    let token = prepare(&h).await;
    let (status, body) = h
        .request(
            "/api/v2/agent/outcomes",
            Some(publication("one")),
            false,
            Some(&token),
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let entry = value(&h, "one").await;
    assert_eq!(entry.verification, OutcomeVerificationState::Unverified);
    let verify = json!({"request_id":"host-one","outcome_id":"one","state":"passed","summary":"Host reviewed the pinned evidence","evidence_indices":[0]});
    let (status, body) = h
        .request_intent(
            "/api/v2/collaboration/outcomes/host-verify",
            Some(verify.clone()),
            true,
            None,
            "manage-outcomes",
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        h.request_intent(
            "/api/v2/collaboration/outcomes/host-verify",
            Some(verify.clone()),
            true,
            None,
            "manage-outcomes"
        )
        .await
        .0,
        200
    );
    let entry = value(&h, "one").await;
    assert_eq!(entry.verification, OutcomeVerificationState::Passed);
    assert_eq!(entry.acceptance, OutcomeAcceptanceState::Pending);
    assert_eq!(
        entry
            .latest_verification
            .as_ref()
            .unwrap()
            .result
            .as_ref()
            .unwrap()
            .evidence,
        entry.outcome.input.evidence
    );
    let decision = json!({"operation":"decide","decision":{"id":"accept-one","outcome_id":"one","verification_id":"host-one","state":OutcomeAcceptanceState::Accepted,"reason":"Host reviewed applicability"}});
    assert_eq!(
        h.request_intent(
            "/api/v2/collaboration/outcomes",
            Some(decision),
            true,
            None,
            "manage-outcomes"
        )
        .await
        .0,
        200
    );
    assert_eq!(
        value(&h, "one").await.acceptance,
        OutcomeAcceptanceState::Accepted
    );
    let mut invalid = verify.clone();
    invalid["request_id"] = json!("host-invalid");
    invalid["evidence_indices"] = json!([99]);
    assert_eq!(
        h.request_intent(
            "/api/v2/collaboration/outcomes/host-verify",
            Some(invalid),
            true,
            None,
            "manage-outcomes"
        )
        .await
        .0,
        400
    );
    let mut invalid = verify;
    invalid["request_id"] = json!("host-unknown");
    invalid["state"] = json!("unknown");
    assert_eq!(
        h.request_intent(
            "/api/v2/collaboration/outcomes/host-verify",
            Some(invalid),
            true,
            None,
            "manage-outcomes"
        )
        .await
        .0,
        400
    );
    assert_eq!(
        h.request_intent(
            "/api/v2/collaboration/outcomes/verify",
            Some(json!({})),
            true,
            None,
            "manage-outcomes"
        )
        .await
        .0,
        404
    );
    let (status, body) = h
        .request(
            "/api/v2/agent/outcomes",
            Some(publication("two")),
            false,
            Some(&token),
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let failed = json!({"request_id":"host-two","outcome_id":"two","state":"failed","summary":"Evidence does not support the conclusion","evidence_indices":[0]});
    assert_eq!(
        h.request_intent(
            "/api/v2/collaboration/outcomes/host-verify",
            Some(failed),
            true,
            None,
            "manage-outcomes"
        )
        .await
        .0,
        200
    );
    let decision = json!({"operation":"decide","decision":{"id":"accept-two","outcome_id":"two","verification_id":"host-two","state":OutcomeAcceptanceState::Accepted,"reason":"Must remain blocked"}});
    assert_eq!(
        h.request_intent(
            "/api/v2/collaboration/outcomes",
            Some(decision),
            true,
            None,
            "manage-outcomes"
        )
        .await
        .0,
        400
    );
    h.close().await;
}

#[tokio::test]
async fn knowledge_scope_bindings_validate_real_groups_and_goals() {
    let root = tempfile::tempdir().unwrap();
    let mut config = service_api::Configuration::defaults(root.path());
    config.services.clear();
    config.adapters.clear();
    config.outcome_services.push(service_api::OutcomeService {
        management: contracts::ServiceManagement::LocalManaged,
        id: "knowledge".into(),
        kind: OutcomeServiceKind::Knowledge,
        endpoint: root.path().join("private/services/knowledge.sock"),
        enabled: false,
        protocol_version: collaboration_model::OUTCOME_PROTOCOL_VERSION,
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
