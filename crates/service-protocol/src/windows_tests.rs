use super::*;
#[tokio::test]
async fn native_pipe_roundtrip_and_exclusive_listener() {
    let state = tempfile::tempdir().unwrap();
    let path = state.path().join("executor.sock");
    let listener = Listener::bind(&path).unwrap();
    assert!(Listener::bind(&path).is_err());
    let server = tokio::spawn(async move {
        let mut stream = listener.accept().await.unwrap();
        let request: crate::Envelope<String> = crate::read(&mut stream).await.unwrap();
        assert_eq!(request.request, "probe");
        crate::write(
            &mut stream,
            &crate::Reply::from_result(Ok(serde_json::json!(42))),
        )
        .await
        .unwrap();
    });
    assert_eq!(
        crate::call::<_, u32>(&path, &"probe", Duration::from_secs(3))
            .await
            .unwrap(),
        42
    );
    server.await.unwrap();
}
#[test]
fn endpoint_names_are_local_and_normalized() {
    assert!(endpoint(Path::new("relative.sock")).is_err());
    assert!(endpoint(Path::new(r"C:\state\..\escape.sock")).is_err());
    assert_eq!(
        name(Path::new(r"C:\State\a.sock")).unwrap(),
        name(Path::new(r"\\?\C:\state\a.sock")).unwrap()
    );
}
