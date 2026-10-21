use super::*;
#[tokio::test]
async fn private_service_diagnostics_never_enter_binding_http_errors() {
    let response = binding_response(Err(anyhow::Error::new(
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
