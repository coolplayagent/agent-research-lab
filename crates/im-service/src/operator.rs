//! Host-only browser bootstrap. The secret lives in masked controller state,
//! never in a worker grant, browser storage, a query string or a public response.
use super::transport::{Web, body, intent, json_response, response};
use crate::*;
use axum::{
    Router,
    body::Body,
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::Response,
    routing::get,
};
use serde::Deserialize;
use std::os::unix::fs::PermissionsExt;
pub struct Operator {
    digest: String,
}
impl Operator {
    pub fn open(state: &Path, addr: SocketAddr) -> Result<Self> {
        let directory = state.join("dashboard");
        fs::create_dir_all(&directory)?;
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
        let _lock = storage::lock(&directory.join("operator.lock"))?;
        let path = directory.join("operator-credential.json");
        let token = match fs::symlink_metadata(&path) {
            Ok(metadata) => {
                ensure!(
                    metadata.is_file()
                        && metadata.permissions().mode() & 0o077 == 0
                        && metadata.len() <= 256,
                    "invalid operator credential file"
                );
                let value: Value = storage::read(&path)?;
                value["token"]
                    .as_str()
                    .context("missing operator token")?
                    .to_owned()
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let mut random = [0u8; 32];
                File::open("/dev/urandom")?.read_exact(&mut random)?;
                let value = storage::digest(&random);
                storage::write(&path, &json!({"token":value}))?;
                value
            }
            Err(e) => return Err(e.into()),
        };
        ensure!(
            token.len() == 64 && token.bytes().all(|b| b.is_ascii_hexdigit()),
            "invalid operator credential"
        );
        storage::write(
            &directory.join("access.json"),
            &json!({"url":format!("http://{addr}/#access={token}")}),
        )?;
        Ok(Self {
            digest: storage::digest(token.as_bytes()),
        })
    }
    fn matches(&self, token: &str) -> bool {
        if token.len() != 64 {
            return false;
        }
        storage::digest(token.as_bytes())
            .bytes()
            .zip(self.digest.bytes())
            .fold(0u8, |difference, (a, b)| difference | (a ^ b))
            == 0
    }
    pub fn authorized(&self, headers: &HeaderMap) -> bool {
        let mut tokens = headers
            .get_all("cookie")
            .iter()
            .filter_map(|v| v.to_str().ok())
            .flat_map(|v| v.split(';'))
            .filter_map(|v| v.trim().strip_prefix("crystal_operator="));
        tokens.next().is_some_and(|value| self.matches(value)) && tokens.next().is_none()
    }
    pub fn fixture(token: &str) -> Self {
        Self {
            digest: storage::digest(token.as_bytes()),
        }
    }
}
pub async fn protect(State(web): State<Web>, request: Request, next: Next) -> Response {
    if super::transport::boundary(request.headers(), web.addr).is_err() {
        return response(
            StatusCode::FORBIDDEN,
            "application/json",
            br#"{"error":"Local origin required"}"#.as_slice(),
        );
    }
    let path = request.uri().path();
    let static_get = request.method() == "GET" && crate::assets::contains(path);
    let actor_api = matches!(
        path,
        "/api/crystal/send"
            | "/api/crystal/stream"
            | "/api/crystal/ack"
            | "/api/crystal/presence"
            | "/api/crystal/inbox"
            | "/api/crystal/history"
            | "/api/im/work"
    );
    if static_get
        || actor_api
        || path == "/api/operator/session"
        || web
            .operator
            .as_ref()
            .is_some_and(|operator| operator.authorized(request.headers()))
    {
        return next.run(request).await;
    }
    response(
        StatusCode::UNAUTHORIZED,
        "application/json",
        br#"{"error":"Host operator login required"}"#.as_slice(),
    )
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Login {
    token: String,
}
pub fn routes() -> Router<Web> {
    Router::new()
        .route("/api/operator/session", get(session).post(login))
        .route("/api/operator/logout", axum::routing::post(logout))
}
async fn session(State(web): State<Web>, headers: HeaderMap) -> Response {
    if web
        .operator
        .as_ref()
        .is_some_and(|operator| operator.authorized(&headers))
    {
        return json_response(Ok(json!({"authenticated":true})));
    }
    response(
        StatusCode::UNAUTHORIZED,
        "application/json",
        br#"{"authenticated":false}"#.as_slice(),
    )
}
async fn login(State(web): State<Web>, request: Request) -> Response {
    let result = async {
        intent(request.headers(), web.addr, "operator-login")?;
        let credentials: Login = serde_json::from_slice(&body(request, 512).await?)?;
        ensure!(
            web.operator
                .as_ref()
                .is_some_and(|operator| operator.matches(&credentials.token)),
            "invalid operator credential"
        );
        Ok::<_, anyhow::Error>(credentials.token)
    }
    .await;
    match result {
        Ok(token) => Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "application/json")
            .header("cache-control", "no-store")
            .header(
                "set-cookie",
                format!(
                    "crystal_operator={token}; HttpOnly; SameSite=Strict; Path=/; Max-Age=86400"
                ),
            )
            .body(Body::from("{\"authenticated\":true}"))
            .unwrap(),
        Err(_) => response(
            StatusCode::UNAUTHORIZED,
            "application/json",
            br#"{"error":"Operator login rejected"}"#.as_slice(),
        ),
    }
}

async fn logout(State(web): State<Web>, request: Request) -> Response {
    if let Err(e) = intent(request.headers(), web.addr, "operator-login") {
        return super::transport::error(e);
    }
    let mut result = json_response(Ok(json!({"authenticated":false})));
    result.headers_mut().insert(
        "set-cookie",
        "crystal_operator=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0"
            .parse()
            .unwrap(),
    );
    result
}
