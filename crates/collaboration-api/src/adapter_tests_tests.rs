use super::*;
#[tokio::test]
async fn private_service_diagnostics_never_enter_adapter_test_http_errors() {
    let response = adapter_test_response(Err(anyhow::Error::new(
        service_protocol::PrivateDiagnostic("fixture private stderr".into()),
    )));
    let body = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    let body = String::from_utf8(body.to_vec()).unwrap();
    assert!(!body.contains("fixture private stderr"));
    assert!(!body.contains("private_details"));
    let value: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert!(value["error"]["code"].is_string());
    assert!(value["error"]["parameters"].is_object());
}

#[tokio::test]
async fn testing_requires_service_intent_but_never_a_digital_person() {
    let mut config =
        service_api::Configuration::defaults(std::path::Path::new("/tmp/adapter-test"));
    for service in &mut config.services {
        service.management = contracts::ServiceManagement::External;
    }
    let harness = crate::tests::Harness::with_services(config).await;
    let endpoint = "/api/v1/collaboration/adapter-tests";
    let body = json!({"request_id":"standalone", "adapter_id":"codex", "revision":1});
    let (status, _) = harness
        .request_intent(endpoint, Some(body.clone()), true, None, "wrong-intent")
        .await;
    assert_eq!(status, 400);
    let (_, records) = harness.request(endpoint, None, true, None).await;
    assert_eq!(serde_json::from_str::<Value>(&records).unwrap(), json!([]));
    let mut forbidden = body.clone();
    forbidden["person_id"] = json!("unrelated-person");
    let (status, _) = harness
        .request_intent(endpoint, Some(forbidden), true, None, "manage-services")
        .await;
    assert_eq!(status, 400);
    let (status, response) = harness
        .request_intent(endpoint, Some(body), true, None, "manage-services")
        .await;
    assert_eq!(status, 200, "{response}");
    let operation: service_api::AdapterTestOperation = serde_json::from_str(&response).unwrap();
    assert_eq!(operation.request.request_id, "standalone");
    harness
        .request_intent(
            "/api/v1/collaboration/adapter-tests/standalone/cancel",
            Some(json!({})),
            true,
            None,
            "manage-services",
        )
        .await;
    let (_, services) = harness
        .request("/api/v1/collaboration/services", None, true, None)
        .await;
    let services: Value = serde_json::from_str(&services).unwrap();
    assert_eq!(services["configuration"]["bindings"], json!({}));
    let (_, people) = harness
        .request("/api/v1/collaboration/people", None, true, None)
        .await;
    let people: Value = serde_json::from_str(&people).unwrap();
    assert_eq!(people["total"], 0);
    harness.close().await;
}
