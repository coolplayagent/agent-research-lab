//! Shared asynchronous transport for the dashboard and digital-person adapters.
use crate::*;
use axum::{
    Router,
    body::{Body, to_bytes},
    extract::{Query, Request, State},
    http::{HeaderMap, StatusCode},
    response::{
        IntoResponse, Response, Sse,
        sse::{Event, KeepAlive},
    },
    routing::{get, post},
};
use crystal::Publish;
use im_storage::Client as Hub;
use serde::Deserialize;
use std::convert::Infallible;
use tokio::sync::{Semaphore, watch};

#[derive(Clone)]
pub struct Web {
    pub services: Option<Arc<service_api::Registry>>,
    pub scenarios: Arc<Vec<Arc<dyn crate::ScenarioAdapter>>>,
    pub hub: Hub,
    pub operator: Option<Arc<super::operator::Operator>>,
    pub addr: SocketAddr,
    pub shutdown: watch::Sender<bool>,
    pub streams: Arc<Semaphore>,
    pub reads: Arc<Semaphore>,
}
impl Web {
    pub fn new(hub: impl Into<Hub>, addr: SocketAddr, shutdown: watch::Sender<bool>) -> Self {
        Self {
            services: None,
            scenarios: Arc::new(vec![Arc::new(crate::Collaboration)]),
            hub: hub.into(),
            operator: None,
            addr,
            shutdown,
            streams: Arc::new(Semaphore::new(20_100)),
            reads: Arc::new(Semaphore::new(16)),
        }
    }
}
pub fn router(web: Web, application: Router<Web>) -> Router {
    Router::new()
        .merge(super::operator::routes())
        .route(
            "/api/im/services",
            get(super::services::view).post(super::services::save),
        )
        .route("/api/crystal/send", post(send))
        .route("/api/crystal/stream", get(stream))
        .route("/api/crystal/ack", post(ack))
        .route("/api/crystal/presence", post(presence))
        .route("/api/crystal/manage", post(super::rooms::manage))
        .route("/api/crystal/view", get(super::rooms::view))
        .route("/api/crystal/events", get(super::rooms::events))
        .route("/api/crystal/inbox", get(crate::agents::inbox))
        .route("/api/crystal/history", get(crate::agents::history))
        .route("/api/im/people", get(crate::agents::people))
        .route("/api/im/scenarios", get(crate::goals::scenarios))
        .route(
            "/api/im/goals",
            get(crate::goals::list).post(crate::goals::manage),
        )
        .route(
            "/api/im/work",
            get(crate::goals::assigned).post(crate::goals::work),
        )
        .merge(application)
        .fallback(resource)
        .layer(axum::middleware::from_fn_with_state(
            web.clone(),
            super::operator::protect,
        ))
        .with_state(web)
}
pub fn boundary(headers: &HeaderMap, addr: SocketAddr) -> Result<()> {
    let mut request = "GET / HTTP/1.1\r\n".to_owned();
    for (key, value) in headers {
        request.push_str(key.as_str());
        request.push_str(": ");
        request.push_str(value.to_str()?);
        request.push_str("\r\n");
    }
    request.push_str("\r\n");
    ensure!(request.len() <= 8192, "headers exceed bound");
    allowed_request(&request, addr)?;
    Ok(())
}
pub fn intent(headers: &HeaderMap, addr: SocketAddr, value: &str) -> Result<()> {
    boundary(headers, addr)?;
    ensure!(
        headers.get_all("origin").iter().count() == 1
            && headers
                .get("content-type")
                .is_some_and(|v| v == "application/json")
            && headers.get("x-crystal-intent").is_some_and(|v| v == value),
        "same-origin JSON intent required"
    );
    Ok(())
}
pub fn token(headers: &HeaderMap) -> Result<&str> {
    ensure!(
        headers.get_all("authorization").iter().count() == 1,
        "credential required"
    );
    headers
        .get("authorization")
        .context("credential required")?
        .to_str()?
        .strip_prefix("Bearer ")
        .context("bearer credential required")
}
pub fn error(error: impl std::fmt::Display) -> Response {
    let text = error.to_string();
    let code = if text.contains("backpressure") {
        StatusCode::TOO_MANY_REQUESTS
    } else if text.contains("writer fault") {
        StatusCode::SERVICE_UNAVAILABLE
    } else {
        StatusCode::BAD_REQUEST
    };
    response(
        code,
        "application/json",
        serde_json::to_vec(&json!({"error":text.chars().take(1000).collect::<String>()})).unwrap(),
    )
}
pub fn response(status: StatusCode, mime: &str, body: impl Into<Body>) -> Response {
    Response::builder().status(status).header("content-type",mime).header("cache-control","no-store")
        .header("x-content-type-options","nosniff").header("referrer-policy","no-referrer")
        .header("content-security-policy","default-src 'self'; connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'")
        .body(body.into()).unwrap()
}
pub fn json_response(result: Result<Value>) -> Response {
    match result {
        Ok(value) => response(
            StatusCode::OK,
            "application/json",
            serde_json::to_vec(&value).unwrap(),
        ),
        Err(e) => error(e),
    }
}
pub async fn body(request: Request, max: usize) -> Result<Vec<u8>> {
    let body =
        tokio::time::timeout(Duration::from_secs(2), to_bytes(request.into_body(), max)).await??;
    Ok(body.to_vec())
}
async fn send(State(web): State<Web>, request: Request) -> Response {
    let result = async {
        boundary(request.headers(), web.addr)?;
        let token = token(request.headers())?.to_owned();
        let input: Publish = serde_json::from_slice(&body(request, 16 * 1024).await?)?;
        Ok(json!(web.hub.publish(&token, input).await?))
    }
    .await;
    json_response(result)
}
#[derive(Deserialize)]
struct Ack {
    group_id: String,
    sequence: u64,
}
async fn ack(State(web): State<Web>, request: Request) -> Response {
    let result = async {
        boundary(request.headers(), web.addr)?;
        let token = token(request.headers())?.to_owned();
        let input: Ack = serde_json::from_slice(&body(request, 2048).await?)?;
        web.hub
            .acknowledge(&token, &input.group_id, input.sequence)
            .await?;
        Ok(json!({"acknowledged":input.sequence}))
    }
    .await;
    json_response(result)
}
#[derive(Deserialize)]
struct StateChange {
    state: crystal::Presence,
}
async fn presence(State(web): State<Web>, request: Request) -> Response {
    let result = async {
        boundary(request.headers(), web.addr)?;
        let actor = web.hub.authenticate(token(request.headers())?).await?;
        let input: StateChange = serde_json::from_slice(&body(request, 1024).await?)?;
        web.hub.set_presence(&actor, input.state).await?;
        Ok(json!(web.hub.presence(&actor).await?))
    }
    .await;
    json_response(result)
}
#[derive(Deserialize)]
struct StreamQuery {
    group_id: String,
    after: Option<u64>,
}
async fn stream(
    State(web): State<Web>,
    headers: HeaderMap,
    Query(query): Query<StreamQuery>,
) -> Response {
    let setup = async {
        boundary(&headers, web.addr)?;
        let token = token(&headers)?.to_owned();
        let actor = web.hub.authenticate(&token).await?;
        let expiry = web.hub.grant_expiry(&token).await?;
        let subscription = web.hub.subscribe(&actor, &query.group_id).await?;
        let after = if let Some(after) = query.after {
            after
        } else if let Some(after) = headers.get("last-event-id") {
            after.to_str()?.parse()?
        } else {
            web.hub.cursor(&actor, &query.group_id).await?
        };
        ensure!(
            after <= web.hub.group(&query.group_id).await?.last_sequence,
            "cursor exceeds conversation history"
        );
        let permit = web
            .streams
            .clone()
            .try_acquire_owned()
            .context("stream backpressure")?;
        Ok::<_, anyhow::Error>((token, expiry, subscription, after, permit))
    }
    .await;
    let (token, expiry, mut subscription, mut cursor, permit) = match setup {
        Ok(v) => v,
        Err(e) => return error(e),
    };
    let mut shutdown = web.shutdown.subscribe();
    let source = async_stream::stream! {
        let _permit=permit;
        yield Ok::<_,Infallible>(Event::default().event("ready").data("{}"));
        let expires=tokio::time::sleep(Duration::from_millis(expiry.saturating_sub(crystal::now_ms())));tokio::pin!(expires);
        let mut replay=true;
        'delivery: loop {
            if *shutdown.borrow() {break;}
            if web.hub.authenticate(&token).await.is_err(){yield Ok(Event::default().event("revoked").data("{}"));break;}
            match subscription.receives().await {
                Err(error)=>{let kind=if error.to_string().contains("writer fault"){"fault"}else{"revoked"};yield Ok(Event::default().event(kind).data("{}"));break;}
                Ok(false)=>{
                    tokio::select!{_ = shutdown.changed()=>break,_=&mut expires=>break,_=subscription.wait_control()=>{}}
                    replay=true;continue;
                }
                Ok(true)=>{}
            }
            if replay {
                let page=match web.hub.history(&query.group_id,cursor,crystal::PAGE_SIZE).await{Ok(p)=>p,Err(e)=>{yield Ok(Event::default().event("fault").data(e.to_string()));break;}};
                // A control operation can run while the disk read is in flight.
                if !subscription.receives().await.unwrap_or(false)||web.hub.authenticate(&token).await.is_err(){continue;}
                subscription.replayed(page.len());
                for message in &page {
                    if !subscription.receives().await.unwrap_or(false)||web.hub.authenticate(&token).await.is_err(){replay=true;continue 'delivery;}
                    cursor=message.sequence;yield Ok(Event::default().event("message").id(cursor.to_string()).json_data(message).unwrap());}
                if page.len()==crystal::PAGE_SIZE {continue;}
                replay=false;
            }
            tokio::select!{
                _=shutdown.changed()=>break,
                _=&mut expires=>break,
                next=subscription.recv()=>match next {
                    Ok(Some(message)) if message.sequence>cursor=>{
                        if !subscription.receives().await.unwrap_or(false)||web.hub.authenticate(&token).await.is_err(){replay=true;continue;}
                        cursor=message.sequence;yield Ok(Event::default().event("message").id(cursor.to_string()).json_data(&*message).unwrap());
                    }
                    Ok(Some(_))=>{},
                    Ok(None)=>replay=true,
                    Err(_)=>break,
                }
            }
        }
    };
    Sse::new(source)
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
        .into_response()
}
async fn resource(State(_web): State<Web>, request: Request) -> Response {
    if request.method() != "GET" {
        return response(StatusCode::METHOD_NOT_ALLOWED, "text/plain", "GET required");
    }
    crate::assets::get(request.uri().path())
}
