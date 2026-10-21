use super::*;
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
