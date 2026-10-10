use super::benchmark::{Events, request};
use super::transport::Web;
use super::*;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::watch,
};
struct Harness {
    hub: crystal::Hub,
    addr: SocketAddr,
    updates: watch::Sender<u64>,
    shutdown: watch::Sender<bool>,
    shared: Shared,
    server: tokio::task::JoinHandle<std::io::Result<()>>,
    path: std::path::PathBuf,
}
impl Harness {
    async fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "crystal-http-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let hub = crystal::Hub::open(&path).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (updates, _) = watch::channel(0);
        let (shutdown, _) = watch::channel(false);
        let shared = Arc::new(RwLock::new(json!({"revision":1,"sample":"first"})));
        let mut web = Web::new(
            hub.clone(),
            addr,
            None,
            shared.clone(),
            updates.clone(),
            shutdown.clone(),
        );
        web.operator = Some(Arc::new(operator::Operator::fixture(&"f".repeat(64))));
        let mut stop = shutdown.subscribe();
        let server = tokio::spawn(async move {
            axum::serve(listener, transport::router(web))
                .with_graceful_shutdown(async move {
                    let _ = stop.changed().await;
                })
                .await
        });
        Self {
            hub,
            addr,
            updates,
            shutdown,
            shared,
            server,
            path,
        }
    }
    async fn raw(&self, header: String) -> String {
        let mut stream = tokio::net::TcpStream::connect(self.addr).await.unwrap();
        let header = header.replacen(
            "\r\n",
            &format!("\r\nCookie: crystal_operator={}\r\n", "f".repeat(64)),
            1,
        );
        stream.write_all(header.as_bytes()).await.unwrap();
        let mut body = String::new();
        tokio::time::timeout(Duration::from_secs(3), stream.read_to_string(&mut body))
            .await
            .unwrap()
            .unwrap();
        body
    }
    async fn close(self) {
        self.shutdown.send_replace(true);
        tokio::time::timeout(Duration::from_secs(3), self.server)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        drop(self.hub);
        fs::remove_dir_all(self.path).unwrap();
    }
}
#[tokio::test]
async fn actual_async_http_enforces_origin_intent_and_resource_boundaries() {
    let h = Harness::new().await;
    for (path, status, content) in [
        ("/", "200 OK", "AI-IM"),
        ("/api/snapshot", "200 OK", "first"),
        ("/../local.toml", "404 Not Found", "Resource unavailable"),
        (
            "/api/session/../../local.toml",
            "404 Not Found",
            "Resource unavailable",
        ),
        (
            "/api/profile/unknown",
            "404 Not Found",
            "Resource unavailable",
        ),
    ] {
        let response = h
            .raw(format!(
                "GET {path} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
                h.addr
            ))
            .await;
        assert!(
            response.starts_with(&format!("HTTP/1.1 {status}")),
            "{response}"
        );
        assert!(response.contains(content));
        assert!(response.contains("frame-ancestors 'none'"));
    }
    for extra in [
        "Host: attacker.example\r\n".to_owned(),
        format!("Host: {}\r\nOrigin: https://evil.invalid\r\n", h.addr),
        format!("Host: {}\r\nSec-Fetch-Site: cross-site\r\n", h.addr),
        format!("Host: {}\r\nHost: {}\r\n", h.addr, h.addr),
    ] {
        let response = h
            .raw(format!(
                "GET /api/snapshot HTTP/1.1\r\n{extra}Connection: close\r\n\r\n"
            ))
            .await;
        assert!(!response.starts_with("HTTP/1.1 200"));
    }
    for extra in [
        "".to_owned(),
        format!("Origin: http://{}\r\n", h.addr),
        "Origin: https://evil.invalid\r\nX-Crystal-Intent: manage-crystal\r\n".to_owned(),
    ] {
        let response=h.raw(format!("POST /api/crystal/manage HTTP/1.1\r\nHost: {}\r\n{extra}Content-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}",h.addr)).await;
        assert!(!response.starts_with("HTTP/1.1 200"));
        assert!(response.contains("same-origin") || response.contains("Local origin"));
    }
    h.close().await;
}
#[tokio::test]
async fn snapshots_push_only_after_notification_while_other_requests_remain_available() {
    let h = Harness::new().await;
    let mut events = Events::open(h.addr, "/api/events", None, Some(&"f".repeat(64)))
        .await
        .unwrap();
    assert_eq!(events.next().await.unwrap().1["sample"], "first");
    let snapshot = h
        .raw(format!(
            "GET /api/snapshot HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
            h.addr
        ))
        .await;
    assert!(snapshot.contains("first"));
    *h.shared.write().unwrap() = json!({"revision":2,"sample":"second"});
    h.updates.send_replace(1);
    assert_eq!(
        tokio::time::timeout(Duration::from_millis(200), events.next())
            .await
            .unwrap()
            .unwrap()
            .1["sample"],
        "second"
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(80), events.next())
            .await
            .is_err()
    );
    drop(events);
    h.close().await;
}
#[tokio::test]
async fn network_busy_queue_reconnect_ack_and_revocation_preserve_private_delivery() {
    let h = Harness::new().await;
    h.hub
        .register(vec!["a".into(), "b".into(), "c".into()])
        .await
        .unwrap();
    h.hub
        .create_group(crystal::NewGroup {
            id: "private".into(),
            title: "Private chat".into(),
            topic: "Joint verification".into(),
            private: true,
            kind: Default::default(),
            members: vec!["a".into(), "b".into()],
        })
        .await
        .unwrap();
    let a = h.hub.grant("a", 3600).await.unwrap();
    assert!(
        request(h.addr, "/api/crystal/view", Some(&a), None)
            .await
            .is_err()
    );
    assert!(
        request(h.addr, "/api/snapshot", Some(&a), None)
            .await
            .is_err()
    );
    let b = h.hub.grant("b", 3600).await.unwrap();
    let c = h.hub.grant("c", 3600).await.unwrap();
    assert!(
        Events::connect(h.addr, "/api/crystal/stream?group_id=private", Some(&c))
            .await
            .is_err()
    );
    let mut stream = Events::connect(h.addr, "/api/crystal/stream?group_id=private", Some(&b))
        .await
        .unwrap();
    assert_eq!(stream.next().await.unwrap().0, "ready");
    request(
        h.addr,
        "/api/crystal/presence",
        Some(&b),
        Some(&json!({"state":"busy"})),
    )
    .await
    .unwrap();
    let message = json!({"group_id":"private","request_id":"one","text":"I have an alternative hypothesis. Let us test it together."});
    let receipt = request(h.addr, "/api/crystal/send", Some(&a), Some(&message))
        .await
        .unwrap();
    let sequence = receipt["message"]["sequence"].as_u64().unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(50), stream.next())
            .await
            .is_err()
    );
    request(
        h.addr,
        "/api/crystal/presence",
        Some(&b),
        Some(&json!({"state":"online"})),
    )
    .await
    .unwrap();
    let received = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(received.1["sequence"], sequence);
    drop(stream);
    // No ACK: reconnect must replay the same durable message.
    let mut stream = Events::connect(h.addr, "/api/crystal/stream?group_id=private", Some(&b))
        .await
        .unwrap();
    stream.next().await.unwrap();
    assert_eq!(stream.next().await.unwrap().1["sequence"], sequence);
    request(
        h.addr,
        "/api/crystal/ack",
        Some(&b),
        Some(&json!({"group_id":"private","sequence":sequence})),
    )
    .await
    .unwrap();
    let _new = h.hub.grant("b", 3600).await.unwrap();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), stream.next())
            .await
            .unwrap()
            .unwrap()
            .0,
        "revoked"
    );
    assert!(
        request(h.addr, "/api/crystal/send", Some(&c), Some(&message))
            .await
            .is_err()
    );
    assert!(
        request(h.addr, "/api/crystal/send", Some(&b), Some(&message))
            .await
            .is_err()
    );
    assert_eq!(h.hub.cursor("b", "private").await.unwrap(), sequence);
    drop(stream);
    h.close().await;
}
