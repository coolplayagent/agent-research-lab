//! Host-only browser access. The direct workspace is bound to loopback and
//! rejects foreign origins; agent bearer grants never become host authority.
use super::transport::{Web, json_response, response};
use crate::*;
use axum::{
    Router,
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::Response,
    routing::get,
};

pub struct Operator;

impl Operator {
    pub fn local(state: &Path, addr: SocketAddr) -> Result<Self> {
        ensure!(
            addr.ip().is_loopback(),
            "direct workspace access requires a loopback address"
        );
        let directory = state.join("dashboard");
        storage::private_dir(&directory)?;
        let _lock = storage::lock(&directory.join("operator.lock"))?;
        storage::write(
            &directory.join("access.json"),
            &json!({"url":format!("http://{addr}/")}),
        )?;
        Ok(Self)
    }

    pub fn authorized(&self, headers: &HeaderMap) -> bool {
        !headers.contains_key("authorization")
    }
}

pub async fn protect(State(web): State<Web>, request: Request, next: Next) -> Response {
    if super::transport::boundary(request.headers(), web.addr).is_err() {
        return response(
            StatusCode::FORBIDDEN,
            "application/json",
            br#"{"error":"Service origin required"}"#.as_slice(),
        );
    }
    if request
        .extensions()
        .get::<axum::extract::MatchedPath>()
        .is_none()
    {
        return next.run(request).await;
    }
    let path = request.uri().path();
    let static_get = request.method() == "GET" && crate::assets::contains(path);
    let actor_api = matches!(
        path,
        "/api/v1/messaging/send"
            | "/api/v1/messaging/stream"
            | "/api/v1/messaging/ack"
            | "/api/v1/messaging/presence"
            | "/api/v1/messaging/inbox"
            | "/api/v1/messaging/history"
            | "/api/v1/collaboration/work"
            | "/api/v1/agent/outcomes"
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
        br#"{"error":"Local operator access required"}"#.as_slice(),
    )
}

pub fn routes() -> Router<Web> {
    Router::new().route("/api/operator/session", get(session))
}

async fn session(State(web): State<Web>, headers: HeaderMap) -> Response {
    if web
        .operator
        .as_ref()
        .is_some_and(|operator| operator.authorized(&headers))
    {
        return json_response(Ok(json!({"authenticated":true,"local_operator":true})));
    }
    response(
        StatusCode::UNAUTHORIZED,
        "application/json",
        br#"{"authenticated":false}"#.as_slice(),
    )
}
