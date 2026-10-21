use super::*;
#[tokio::test]
async fn knowledge_http_errors_are_structured_and_exclude_transport_text() {
    let diagnostic = contracts::KnowledgeError::new(contracts::KnowledgeErrorCode::DocumentLimit)
        .parameter("path", "large.pdf")
        .parameter("actual", 2048)
        .parameter("limit", 1024);
    let error = service_protocol::Reply::from_result(Err(diagnostic.into()))
        .decode::<bool>()
        .unwrap_err();
    let response = knowledge_response(Err(error));
    let bytes = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["error"]["code"], "knowledge_document_limit");
    assert_eq!(value["error"]["parameters"]["path"], "large.pdf");
    assert!(
        !String::from_utf8(bytes.to_vec())
            .unwrap()
            .contains("service request failed")
    );
    let response = knowledge_response(Err(anyhow::anyhow!("private transport detail")));
    let bytes = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    assert!(
        !String::from_utf8(bytes.to_vec())
            .unwrap()
            .contains("private transport detail")
    );
}
