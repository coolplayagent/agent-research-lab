use crate::tests::Harness;
use serde_json::{Value, json};
#[tokio::test]
async fn group_goal_http_flow_is_independent_of_research_and_enforces_operator_and_assignment_roles()
 {
    let h = Harness::new().await;
    for id in ["planner", "reviewer", "outsider"] {
        h.manage(json!({"operation":"person","person":{"id":id,"name":id}}))
            .await;
    }
    h.manage(json!({"operation":"create","group":{"id":"team","kind":"conversation","title":"Design","topic":"Compare alternatives","members":["planner","reviewer"]}})).await;
    let create = json!({"operation":"create","goal":{"id":"goal","group_id":"team","title":"Choose a protocol","objective":"Compare two designs","acceptance":"Both members report with evidence","scenario":"collaboration","parameters":{},"max_seconds":60,"assignments":[{"person_id":"planner","instruction":"Propose two designs"},{"person_id":"reviewer","instruction":"Review the tradeoffs"}]}});
    assert_eq!(
        h.request("/api/im/goals", Some(create.clone()), false, None)
            .await
            .0,
        401
    );
    let (status, body) = h
        .request("/api/im/goals", Some(create.clone()), true, None)
        .await;
    assert_eq!(status, 200, "{body}");
    let mut invalid = create;
    invalid["goal"]["id"] = json!("invalid");
    invalid["goal"]["scenario"] = json!("not-installed");
    assert_eq!(
        h.request("/api/im/goals", Some(invalid), true, None)
            .await
            .0,
        400
    );
    let mut revision = 0;
    for (index, id) in ["planner", "reviewer"].into_iter().enumerate() {
        let grant = h
            .manage(json!({"operation":"grant","person_id":id,"lifetime_seconds":60}))
            .await;
        let token = grant["token"].as_str().unwrap();
        let (status, body) = h.request("/api/im/work", None, false, Some(token)).await;
        assert_eq!(status, 200, "{body}");
        let page: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(
            page["goals"][0]["input"]["objective"],
            "Compare two designs"
        );
        assert_eq!(
            h.request("/api/im/goals", None, false, Some(token)).await.0,
            401
        );
        let claim = json!({"operation":"claim","goal_id":"goal","work_id":format!("work-{index}"),"claim_id":id,"attempt":1});
        let response = h
            .request("/api/im/work", Some(claim), false, Some(token))
            .await;
        assert_eq!(response.0, 200, "{}", response.1);
        let submit = json!({"operation":"submit","goal_id":"goal","work_id":format!("work-{index}"),"claim_id":id,"attempt":1,"result":{"summary":"Compared alternatives","succeeded":true,"evidence":{"artifact":"report"}}});
        let response = h
            .request("/api/im/work", Some(submit), false, Some(token))
            .await;
        assert_eq!(response.0, 200, "{}", response.1);
        let goal: Value = serde_json::from_str(&response.1).unwrap();
        assert_eq!(goal["state"], "active");
        revision = goal["revision"].as_u64().unwrap();
        assert_eq!(
            h.request(
                "/api/im/goals",
                Some(json!({"operation":"accept","goal_id":"goal","revision":revision})),
                false,
                Some(token)
            )
            .await
            .0,
            401
        );
    }
    let response = h
        .request(
            "/api/im/goals",
            Some(json!({"operation":"accept","goal_id":"goal","revision":revision})),
            true,
            None,
        )
        .await;
    assert_eq!(response.0, 200, "{}", response.1);
    assert_eq!(
        serde_json::from_str::<Value>(&response.1).unwrap()["state"],
        "completed"
    );
    h.close().await;
}

#[tokio::test]
async fn mention_dispatch_is_operator_only_and_rejects_unavailable_scenarios_without_sending() {
    let h = Harness::new().await;
    h.manage(json!({"operation":"person","person":{"id":"planner","name":"Planner"}}))
        .await;
    h.manage(json!({"operation":"create","group":{"id":"team","kind":"conversation","title":"Design","topic":"Discussion","members":["planner"]}})).await;
    let mut request = json!({"message":{"group_id":"team","request_id":"ask","text":"@Planner Review this","reply_to":null},"goal":{"id":"client-placeholder","group_id":"team","title":"Review","objective":"@Planner Review this","acceptance":"Respond with evidence","scenario":"missing","parameters":{},"max_seconds":60,"assignments":[{"person_id":"planner","instruction":"Reply to the question"}]}});
    assert_eq!(
        h.request("/api/im/discussion", Some(request.clone()), false, None)
            .await
            .0,
        401
    );
    assert_eq!(
        h.request("/api/im/discussion", Some(request.clone()), true, None)
            .await
            .0,
        400
    );
    request["goal"]["scenario"] = json!("collaboration");
    let (status, body) = h
        .request("/api/im/discussion", Some(request.clone()), true, None)
        .await;
    assert_eq!(status, 200, "{body}");
    let first: Value = serde_json::from_str(&body).unwrap();
    assert!(
        first["goal"]["input"]["id"]
            .as_str()
            .unwrap()
            .starts_with("mention-")
    );
    let (_, body) = h
        .request("/api/im/discussion", Some(request), true, None)
        .await;
    let again: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(first["message"]["sequence"], again["message"]["sequence"]);
    assert_eq!(again["duplicate"], true);
    let (_, body) = h
        .request("/api/crystal/view?group_id=team", None, true, None)
        .await;
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap()["messages"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    h.close().await;
}
