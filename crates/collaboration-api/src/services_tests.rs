use super::*;
#[tokio::test]
async fn cleanup_requires_current_configuration_and_keeps_bound_directory_entries() {
    let mut config =
        service_api::Configuration::defaults(std::path::Path::new("/tmp/cleanup-test"));
    for service in &mut config.services {
        service.management = contracts::ServiceManagement::External;
    }
    config.bindings.insert("bound".into(), "codex".into());
    let harness = crate::tests::Harness::with_services(config).await;
    for id in ["bound", "extra"] {
        harness
            .hub
            .save_person(collaboration_model::Person {
                id: id.into(),
                name: id.into(),
                application_id: None,
            })
            .await
            .unwrap();
    }
    let endpoint = "/api/v1/collaboration/people/cleanup";
    let (status, _) = harness
        .request_intent(
            endpoint,
            Some(json!({"revision":1})),
            true,
            None,
            "wrong-intent",
        )
        .await;
    assert_eq!(status, 400);
    let (status, _) = harness
        .request_intent(
            endpoint,
            Some(json!({"revision":0})),
            true,
            None,
            "manage-services",
        )
        .await;
    assert_eq!(status, 409);
    let (status, body) = harness
        .request_intent(
            endpoint,
            Some(json!({"revision":1})),
            true,
            None,
            "manage-services",
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&body).unwrap()["archived"],
        1
    );
    let (status, body) = harness
        .request("/api/v1/collaboration/people", None, true, None)
        .await;
    assert_eq!(status, 200);
    let people: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(people["total"], 1);
    harness.close().await;
}
#[tokio::test]
async fn configuration_failures_have_stable_structured_codes() {
    let response = settings_response(Err(service_api::ConfigurationFailure(
        contracts::ServiceConfigurationErrorCode::CommandRequired,
    )
    .into()));
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let bytes = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["error"]["code"], "adapter_command_required");
    assert!(value["error"]["parameters"].is_object());
}
#[tokio::test]
async fn runtime_failures_do_not_expose_host_diagnostics() {
    let response = settings_response(Err(anyhow::anyhow!("private diagnostic content").context(
        service_api::ConfigurationFailure(
            contracts::ServiceConfigurationErrorCode::RuntimeUnavailable,
        ),
    )));
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let bytes = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    assert!(
        !String::from_utf8(bytes.to_vec())
            .unwrap()
            .contains("private diagnostic")
    );
}
