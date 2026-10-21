use super::*;
#[test]
fn operating_system_error_survives_protocol_and_context_wrapping() {
    let error = anyhow::Error::from(std::io::Error::from_raw_os_error(1))
        .context("flush private state file");
    let encoded = serde_json::to_vec(&Reply::from_result(Err(error))).unwrap();
    let decoded: Reply = serde_json::from_slice(&encoded).unwrap();
    let error = decoded.decode::<serde_json::Value>().unwrap_err();
    assert_eq!(os_code(&error), Some(1));
    assert!(error.to_string().contains("flush private state file"));
}
#[tokio::test]
async fn framed_calls_validate_version_and_private_peer() {
    let root = tempfile::tempdir().unwrap();
    let socket = root.path().join("private/service.sock");
    let listener = Listener::bind(&socket).unwrap();
    let task = tokio::spawn(async move {
        let mut stream = listener.accept().await.unwrap();
        let envelope: Envelope<String> = read(&mut stream).await.unwrap();
        assert_eq!(envelope.request, "probe");
        write(&mut stream, &Reply::from_result(Ok(serde_json::json!(42))))
            .await
            .unwrap();
    });
    assert_eq!(
        call::<_, u32>(&socket, &"probe", Duration::from_secs(2))
            .await
            .unwrap(),
        42
    );
    task.await.unwrap();
    assert!(
        Reply {
            error_os_code: None,
            error_private_details: None,
            error_knowledge: None,
            protocol_version: 99,
            result: serde_json::json!(true),
            error: None
        }
        .decode::<bool>()
        .is_err()
    );
}

#[test]
fn private_diagnostics_survive_service_hops_without_entering_display() {
    let error =
        anyhow::Error::new(PrivateDiagnostic("fixture diagnostic".into())).context("execute");
    let first = Reply::from_result(Err(error)).decode::<bool>().unwrap_err();
    let second = Reply::from_result(Err(first)).decode::<bool>().unwrap_err();
    assert_eq!(
        private_details(&second).as_deref(),
        Some("fixture diagnostic")
    );
    assert!(!format!("{second:#}").contains("fixture diagnostic"));
    let legacy: Reply = serde_json::from_value(
        serde_json::json!({"protocol_version":1,"result":null,"error":"legacy"}),
    )
    .unwrap();
    assert!(legacy.error_private_details.is_none());
}

#[test]
fn structured_knowledge_errors_survive_multiple_service_hops() {
    let diagnostic = contracts::KnowledgeError::new(contracts::KnowledgeErrorCode::DocumentLimit)
        .parameter("path", "large.pdf")
        .parameter("limit", 1024);
    let first = Reply::from_result(Err(diagnostic.into()))
        .decode::<bool>()
        .unwrap_err();
    let second = Reply::from_result(Err(first)).decode::<bool>().unwrap_err();
    let diagnostic = knowledge_error(&second).unwrap();
    assert_eq!(
        diagnostic.code,
        contracts::KnowledgeErrorCode::DocumentLimit
    );
    assert_eq!(diagnostic.parameters["path"], "large.pdf");
}
