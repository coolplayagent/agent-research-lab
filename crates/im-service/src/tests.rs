use crate::*;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::watch,
};
pub(super) struct Harness {
    _state: tempfile::TempDir,
    hub: crystal::Hub,
    addr: SocketAddr,
    stop: watch::Sender<bool>,
    server: tokio::task::JoinHandle<std::io::Result<()>>,
}
impl Harness {
    pub(super) async fn new() -> Self {
        let state = tempfile::tempdir().unwrap();
        let hub = crystal::Hub::open(state.path()).unwrap();
        hub.register_operator().await.unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (stop, mut stopped) = watch::channel(false);
        let mut web = transport::Web::new(hub.clone(), addr, stop.clone());
        web.operator = Some(Arc::new(operator::Operator::fixture(&"f".repeat(64))));
        let server = tokio::spawn(async move {
            axum::serve(listener, transport::router(web, axum::Router::new()))
                .with_graceful_shutdown(async move {
                    let _ = stopped.changed().await;
                })
                .await
        });
        Self {
            _state: state,
            hub,
            addr,
            stop,
            server,
        }
    }
    pub(super) async fn request(
        &self,
        path: &str,
        body: Option<Value>,
        operator: bool,
        token: Option<&str>,
    ) -> (u16, String) {
        let mut stream = tokio::net::TcpStream::connect(self.addr).await.unwrap();
        let method = if body.is_some() { "POST" } else { "GET" };
        let payload = body.map(|v| v.to_string()).unwrap_or_default();
        let cookie = if operator {
            format!("Cookie: crystal_operator={}\r\n", "f".repeat(64))
        } else {
            String::new()
        };
        let auth = token
            .map(|t| format!("Authorization: Bearer {t}\r\n"))
            .unwrap_or_default();
        let request = format!(
            "{method} {path} HTTP/1.1\r\nHost: {}\r\nOrigin: http://{}\r\nX-Crystal-Intent: manage-crystal\r\n{cookie}{auth}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
            self.addr,
            self.addr,
            payload.len()
        );
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut response = String::new();
        tokio::time::timeout(Duration::from_secs(3), stream.read_to_string(&mut response))
            .await
            .unwrap()
            .unwrap();
        let status = response.split_whitespace().nth(1).unwrap().parse().unwrap();
        (status, response.split_once("\r\n\r\n").unwrap().1.into())
    }
    pub(super) async fn manage(&self, body: Value) -> Value {
        let (status, response) = self
            .request("/api/crystal/manage", Some(body), true, None)
            .await;
        assert_eq!(status, 200, "{response}");
        serde_json::from_str(&response).unwrap()
    }
    pub(super) async fn close(self) {
        self.stop.send_replace(true);
        tokio::time::timeout(Duration::from_secs(3), self.server)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }
}
#[tokio::test]
async fn independent_host_manages_agents_groups_dms_boards_without_research_configuration() {
    let h = Harness::new().await;
    assert!(
        h.request("/", None, false, None)
            .await
            .1
            .contains("水晶球公告板")
    );
    assert_eq!(h.request("/api/im/people", None, false, None).await.0, 401);
    assert_eq!(h.request("/api/snapshot", None, true, None).await.0, 404);
    let response = h.request("/api/im/scenarios", None, true, None).await;
    assert_eq!(response.0, 200);
    let scenarios: Value = serde_json::from_str(&response.1).unwrap();
    assert_eq!(scenarios["scenarios"][0]["id"], "collaboration");
    for id in ["a", "b", "c"] {
        h.manage(json!({"operation":"person","person":{"id":id,"name":format!("Agent {id}")}}))
            .await;
    }
    let group = h.manage(json!({"operation":"create","group":{"id":"team","kind":"conversation","title":"Team","topic":"Work together","members":["a"]}})).await;
    assert_eq!(group["member_count"], 2);
    h.manage(json!({"operation":"membership","group_id":"team","person_id":"b","add":true}))
        .await;
    let receipt = h.manage(json!({"operation":"send","message":{"group_id":"team","request_id":"one","text":"Welcome"}})).await;
    assert_eq!(receipt["message"]["sender_id"], "operator");
    let dm = h.manage(json!({"operation":"create","group":{"id":"dm","kind":"temporary","private":true,"title":"A and B","topic":"Quick discussion","members":["a","b"]}})).await;
    assert_eq!(dm["kind"], "temporary");
    h.manage(json!({"operation":"fork","source":"dm","history_from":null,"group":{"id":"new-dm","kind":"temporary","private":true,"title":"New audience","topic":"Quick discussion","members":["operator","a","b","c"]}})).await;
    h.manage(
        json!({"operation":"convert","group_id":"dm","revision":1,"title":"Long running project"}),
    )
    .await;
    assert!(!h.hub.group("dm").unwrap().private);
    h.manage(json!({"operation":"create","group":{"id":"board","kind":"board","title":"Announcements","topic":"Milestones","members":["a"]}})).await;
    h.manage(json!({"operation":"send","message":{"group_id":"board","request_id":"announcement","text":"Ship it"}})).await;
    let grant = h
        .manage(json!({"operation":"grant","person_id":"a","lifetime_seconds":60}))
        .await;
    let token = grant["token"].as_str().unwrap();
    assert_eq!(
        h.request(
            "/api/crystal/manage",
            Some(json!({"operation":"membership","group_id":"board","person_id":"c","add":true})),
            false,
            Some(token)
        )
        .await
        .0,
        401
    );
    assert_eq!(
        h.request(
            "/api/crystal/send",
            Some(json!({"group_id":"board","request_id":"forbidden","text":"Not an operator"})),
            false,
            Some(token)
        )
        .await
        .0,
        400
    );
    assert!(
        h.request(
            "/api/crystal/history?group_id=board",
            None,
            false,
            Some(token)
        )
        .await
        .1
        .contains("Ship it")
    );
    let outsider = h.hub.grant("c", 60).await.unwrap();
    assert_eq!(
        h.request(
            "/api/crystal/history?group_id=board",
            None,
            false,
            Some(&outsider)
        )
        .await
        .0,
        400
    );
    assert!(
        !h.request("/api/crystal/inbox", None, false, Some(&outsider))
            .await
            .1
            .contains("Announcements")
    );
    h.close().await;
}
