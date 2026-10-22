use crate::tests::Harness;
use crate::*;
#[tokio::test]
async fn task_http_separates_operator_intent_from_execution_authority() {
    let h = Harness::new().await;
    h.manage(json!({"operation":"person","person":{"id":"worker","name":"Worker"}}))
        .await;
    h.manage(json!({"operation":"create","group":{"id":"team","title":"Team","topic":"Work","private":true,"members":["worker"]}})).await;
    let create = json!({"operation":"create","input":{"id":"task","group_id":"team","intent":"Prepare a deliverable"}});
    let endpoint = "/api/v2/collaboration/tasks";
    let (status, body) = h
        .request_intent(endpoint, Some(create.clone()), true, None, "manage-crystal")
        .await;
    assert_eq!(status, 200, "{body}");
    let token = h.hub.grant("worker", 60).await.unwrap();
    assert_eq!(h.request(endpoint, None, false, Some(&token)).await.0, 401);
    assert_ne!(
        h.request_intent(endpoint, Some(create), true, None, "other")
            .await
            .0,
        200
    );
    for action in [
        json!({"operation":"claim","node_id":null,"claim_id":"forged"}),
        json!({"operation":"pause","code":"task_executor_unavailable"}),
    ] {
        let (status, _) = h
            .request_intent(
                endpoint,
                Some(json!({"operation":"change","id":"task","revision":1,"action":action})),
                true,
                None,
                "manage-crystal",
            )
            .await;
        assert_eq!(status, 403);
    }
    let (_,body)=h.request_intent(endpoint,Some(json!({"operation":"change","id":"task","revision":1,"action":{"operation":"cancel"}})),true,None,"manage-crystal").await;
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap()["state"],
        "cancelled"
    );
    h.close().await;
}
