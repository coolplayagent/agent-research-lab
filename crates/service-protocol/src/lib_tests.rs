use super::*;
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
            protocol_version: 99,
            result: serde_json::json!(true),
            error: None
        }
        .decode::<bool>()
        .is_err()
    );
}
