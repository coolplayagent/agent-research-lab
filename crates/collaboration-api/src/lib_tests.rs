use crate::*;
use std::fs;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::watch,
};
#[tokio::test]
async fn errors_preserve_messages_and_add_stable_codes_for_existing_statuses() {
    for (message, status, code) in [
        (
            "invalid payload",
            400,
            contracts::ApiErrorCode::InvalidRequest,
        ),
        (
            "execution backpressure",
            429,
            contracts::ApiErrorCode::Backpressure,
        ),
        (
            "storage writer fault",
            503,
            contracts::ApiErrorCode::StorageUnavailable,
        ),
    ] {
        let response = transport::error(message);
        assert_eq!(response.status().as_u16(), status);
        let body = axum::body::to_bytes(response.into_body(), 2048)
            .await
            .unwrap();
        let value: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["code"], code.as_str());
        assert_eq!(value["error"], message);
    }
}
#[test]
fn direct_workspace_requires_loopback_and_exact_service_origin() {
    let addr: SocketAddr = "127.0.0.1:8090".parse().unwrap();
    assert!(validate_listen(addr).is_ok());
    for address in [
        "0.0.0.0:8090",
        "[::]:8090",
        "224.0.0.1:8090",
        "100.125.79.91:8090",
    ] {
        assert!(validate_listen(address.parse().unwrap()).is_err());
    }
    let request = |host: &str, origin: &str| {
        format!("GET / HTTP/1.1\r\nHost: {host}\r\nOrigin: {origin}\r\n\r\n")
    };
    assert!(allowed_request(&request("127.0.0.1:8090", "http://127.0.0.1:8090"), addr).is_ok());
    for (host, origin) in [
        ("evil.example:8090", "http://127.0.0.1:8090"),
        ("127.0.0.1:8090", "http://foreign.invalid"),
    ] {
        assert!(allowed_request(&request(host, origin), addr).is_err());
    }
    let state = tempfile::tempdir().unwrap();
    let operator = operator::Operator::local(state.path(), addr).unwrap();
    assert!(operator.authorized(&axum::http::HeaderMap::new()));
    let access: Value = storage::read(&state.path().join("dashboard/access.json")).unwrap();
    assert_eq!(access["url"], "http://127.0.0.1:8090/");
}
pub(super) struct Harness {
    _state: tempfile::TempDir,
    pub(super) hub: collaboration_store::Hub,
    addr: SocketAddr,
    stop: watch::Sender<bool>,
    server: tokio::task::JoinHandle<std::io::Result<()>>,
}
impl Harness {
    pub(super) async fn new() -> Self {
        Self::configured(None).await
    }
    pub(super) async fn with_services(config: service_api::Configuration) -> Self {
        Self::configured(Some(config)).await
    }
    async fn configured(config: Option<service_api::Configuration>) -> Self {
        let state = tempfile::tempdir().unwrap();
        let hub = collaboration_store::Hub::open(state.path()).unwrap();
        hub.register_operator().await.unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (stop, mut stopped) = watch::channel(false);
        let mut web = transport::Web::new(
            collaboration_storage_service::client(hub.clone()),
            addr,
            stop.clone(),
        );
        if let Some(mut config) = config {
            // Managed knowledge endpoints are owned by this harness's private state.
            for service in &mut config.outcome_services {
                if service.management == contracts::ServiceManagement::LocalManaged {
                    service.endpoint = state
                        .path()
                        .join(format!("private/services/{}.sock", service.id));
                }
            }
            let registry = service_registry::Registry::open(state.path()).unwrap();
            registry.save(config).unwrap();
            web.services = Some(Arc::new(registry));
        }
        web.operator = Some(Arc::new(
            operator::Operator::local(state.path(), addr).unwrap(),
        ));
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
        self.request_intent(path, body, operator, token, "manage-crystal")
            .await
    }
    pub(super) async fn request_intent(
        &self,
        path: &str,
        body: Option<Value>,
        operator: bool,
        token: Option<&str>,
        intent: &str,
    ) -> (u16, String) {
        let mut stream = tokio::net::TcpStream::connect(self.addr).await.unwrap();
        let method = if body.is_some() { "POST" } else { "GET" };
        let payload = body.map(|v| v.to_string()).unwrap_or_default();
        let _ = operator;
        let auth = token
            .map(|t| format!("Authorization: Bearer {t}\r\n"))
            .unwrap_or_default();
        let request = format!(
            "{method} {path} HTTP/1.1\r\nHost: {}\r\nOrigin: http://{}\r\nX-Crystal-Intent: {intent}\r\n{auth}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
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
            .request("/api/v1/messaging/manage", Some(body), true, None)
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
async fn bootstrap_module_graph_is_publicly_served() {
    let h = Harness::new().await;
    let mut pending = vec!["/assets/bootstrap.js".to_owned()];
    let mut visited = std::collections::BTreeSet::new();
    while let Some(path) = pending.pop() {
        if !visited.insert(path.clone()) {
            continue;
        }
        let (status, body) = h.request(&path, None, false, None).await;
        assert_eq!(status, 200, "Unavailable module {path}: {body}");
        for line in body.lines().filter(|line| line.starts_with("import ")) {
            let import = line.split('"').nth(1).expect("literal module import");
            let target = if import.starts_with('/') {
                import.to_owned()
            } else {
                let relative = import.strip_prefix("./").expect("local module import");
                format!("{}/{relative}", path.rsplit_once('/').unwrap().0)
            };
            pending.push(target);
        }
    }
    assert!(
        visited.len() > 10,
        "Expected the complete workspace module graph"
    );
    h.close().await;
}
#[tokio::test]
async fn independent_host_manages_agents_groups_dms_boards_without_research_configuration() {
    let h = Harness::new().await;
    let (status, html) = h.request("/", None, false, None).await;
    assert_eq!(status, 200);
    assert!(html.contains("id=\"workspace\""));
    assert!(html.contains("/assets/bootstrap.js"));
    for (path, content) in [
        ("/assets/experience.js", "export"),
        ("/assets/feedback.js", "export"),
        ("/favicon.svg", "<svg"),
    ] {
        let (status, body) = h.request(path, None, false, None).await;
        assert_eq!(status, 200, "{path}: {body}");
        assert!(body.contains(content), "{path}: {body}");
    }
    assert_eq!(
        h.request("/api/v1/collaboration/people", None, false, None)
            .await
            .0,
        200
    );
    assert_eq!(h.request("/api/snapshot", None, true, None).await.0, 404);
    let response = h
        .request("/api/v1/collaboration/scenarios", None, true, None)
        .await;
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
            "/api/v1/messaging/manage",
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
            "/api/v1/messaging/send",
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
            "/api/v1/messaging/history?group_id=board",
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
            "/api/v1/messaging/history?group_id=board",
            None,
            false,
            Some(&outsider)
        )
        .await
        .0,
        400
    );
    assert!(
        !h.request("/api/v1/messaging/inbox", None, false, Some(&outsider))
            .await
            .1
            .contains("Announcements")
    );
    h.close().await;
}

#[tokio::test]
async fn local_operator_enters_management_without_changing_actor_grants() {
    let h = Harness::new().await;
    let (status, session) = h.request("/api/operator/session", None, false, None).await;
    assert_eq!(status, 200);
    let session: Value = serde_json::from_str(&session).unwrap();
    assert_eq!(session["authenticated"], true);
    assert_eq!(session["local_operator"], true);
    assert_eq!(
        h.request("/api/v1/collaboration/people", None, false, None)
            .await
            .0,
        200
    );
    let (status, response) = h.request("/api/v1/messaging/manage",Some(json!({"operation":"person","person":{"id":"worker","name":"Worker","application_id":null}})),false,None).await;
    assert_eq!(status, 200, "{response}");
    let grant = h
        .manage(json!({"operation":"grant","person_id":"worker","lifetime_seconds":60}))
        .await;
    let token = grant["token"].as_str().unwrap();
    assert_eq!(
        h.request("/api/v1/collaboration/people", None, false, Some(token))
            .await
            .0,
        401
    );
    assert_eq!(h.request("/api/v1/messaging/manage",Some(json!({"operation":"person","person":{"id":"imposter","name":"Imposter","application_id":null}})),false,Some(token)).await.0,401);
    assert_ne!(
        h.request("/api/v1/messaging/inbox", None, false, None)
            .await
            .0,
        200
    );
    assert_ne!(
        h.request("/api/v1/messaging/inbox", None, false, Some("wrong-token"))
            .await
            .0,
        200
    );
    assert_eq!(
        h.request("/api/v1/messaging/inbox", None, false, Some(token))
            .await
            .0,
        200
    );
    for headers in [
        "Host: evil.example\r\n".to_owned(),
        format!("Host: {}\r\nOrigin: https://evil.invalid\r\n", h.addr),
        format!("Host: {}\r\nSec-Fetch-Site: cross-site\r\n", h.addr),
    ] {
        let mut stream = tokio::net::TcpStream::connect(h.addr).await.unwrap();
        stream
            .write_all(
                format!("GET /api/operator/session HTTP/1.1\r\n{headers}Connection: close\r\n\r\n")
                    .as_bytes(),
            )
            .await
            .unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).await.unwrap();
        assert!(response.starts_with("HTTP/1.1 403"));
    }
    assert!(
        !h._state
            .path()
            .join("dashboard/operator-credential.json")
            .exists()
    );
    let access: Value = storage::read(&h._state.path().join("dashboard/access.json")).unwrap();
    assert_eq!(access["url"], format!("http://{}/", h.addr));
    assert_eq!(
        h.request("/api/operator/session", Some(json!({})), false, None)
            .await
            .0,
        405
    );
    assert_eq!(
        h.request("/api/operator/logout", Some(json!({})), false, None)
            .await
            .0,
        404
    );
    h.close().await;
}
#[test]
fn direct_workspace_rejects_remote_bind_and_ignores_legacy_credential() {
    let state = tempfile::tempdir().unwrap();
    assert!(operator::Operator::local(state.path(), "0.0.0.0:8090".parse().unwrap()).is_err());
    let addr = "127.0.0.1:8090".parse().unwrap();
    fs::create_dir_all(state.path().join("dashboard")).unwrap();
    let legacy = state.path().join("dashboard/operator-credential.json");
    fs::write(&legacy, b"legacy credential stays untouched").unwrap();
    operator::Operator::local(state.path(), addr).unwrap();
    assert_eq!(
        fs::read(&legacy).unwrap(),
        b"legacy credential stays untouched"
    );
    let headers = axum::http::HeaderMap::new();
    assert!(
        operator::Operator::local(state.path(), addr)
            .unwrap()
            .authorized(&headers)
    );
    let access: Value = storage::read(&state.path().join("dashboard/access.json")).unwrap();
    assert_eq!(access["url"], "http://127.0.0.1:8090/");
}

#[tokio::test]
async fn canonical_routes_preserve_operator_and_actor_authority() {
    let h = Harness::new().await;
    for (old, new, expected) in [
        ("/api/im/people", "/api/v1/collaboration/people", 200),
        ("/api/crystal/view", "/api/v1/messaging/view", 200),
        ("/api/im/outcomes", "/api/v1/collaboration/outcomes", 404),
        ("/api/im/topics", "/api/v1/collaboration/topics", 200),
    ] {
        for operator in [false, true] {
            assert_eq!(h.request(old, None, operator, None).await.0, 404);
        }
        assert_eq!(h.request(new, None, false, None).await.0, expected, "{new}");
        assert_eq!(h.request(new, None, true, None).await.0, expected, "{new}");
    }
    for (old, new) in [
        ("/api/crystal/send", "/api/v1/messaging/send"),
        ("/api/im/work", "/api/v1/collaboration/work"),
        ("/api/crystal/outcomes", "/api/v1/agent/outcomes"),
    ] {
        assert_eq!(h.request(old, Some(json!({})), false, None).await.0, 404);
        assert_ne!(h.request(new, Some(json!({})), false, None).await.0, 200);
    }
    assert_eq!(
        h.request_intent(
            "/api/v1/knowledge",
            Some(json!({})),
            false,
            None,
            "manage-knowledge"
        )
        .await
        .0,
        400
    );
    assert_ne!(
        h.request_intent(
            "/api/v1/knowledge",
            Some(json!({})),
            true,
            None,
            "manage-crystal"
        )
        .await
        .0,
        200
    );
    h.close().await;
}

#[path = "goal_tests.rs"]
mod goal_tests;

#[path = "iteration_tests.rs"]
mod iteration_tests;

#[path = "outcome_tests.rs"]
mod outcome_tests;

#[tokio::test]
async fn deletion_requires_host_intent_and_preserves_old_message_wire_shape() {
    let h = Harness::new().await;
    h.manage(json!({"operation":"person","person":{"id":"worker","name":"Worker","application_id":null}})).await;
    h.manage(json!({"operation":"create","group":{"id":"board","kind":"board","title":"Board","topic":"News","members":["worker"]}})).await;
    let receipt = h.manage(json!({"operation":"send","message":{"group_id":"board","request_id":"post","text":"Remove this announcement"}})).await;
    assert!(receipt["message"].get("deleted").is_none());
    let target = json!({"operation":"delete","target":{"kind":"announcement","group_id":"board","sequence":receipt["message"]["sequence"],"revision":1}});
    let grant = h
        .manage(json!({"operation":"grant","person_id":"worker","lifetime_seconds":60}))
        .await;
    assert_eq!(
        h.request(
            "/api/v1/messaging/manage",
            Some(target.clone()),
            false,
            grant["token"].as_str()
        )
        .await
        .0,
        401
    );
    assert_ne!(
        h.request_intent(
            "/api/v1/messaging/manage",
            Some(target.clone()),
            true,
            None,
            "wrong-intent"
        )
        .await
        .0,
        200
    );
    assert_eq!(h.manage(target.clone()).await["code"], "deleted");
    assert_eq!(h.manage(target).await["code"], "revision_conflict");
    let (status, history) = h
        .request("/api/v1/messaging/view?group_id=board", None, true, None)
        .await;
    assert_eq!(status, 200);
    let history: Value = serde_json::from_str(&history).unwrap();
    assert_eq!(history["messages"][0]["deleted"], true);
    assert_eq!(history["messages"][0]["text"], "");
    h.close().await;
}
