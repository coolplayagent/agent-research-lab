//! Shared asynchronous transport for the dashboard and digital-person adapters.
use super::*;
use axum::{
    Router,
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    response::{
        IntoResponse, Response, Sse,
        sse::{Event, KeepAlive},
    },
    routing::get,
};
use crystal::Hub;
use std::convert::Infallible;
use tokio::sync::{Semaphore, watch};

#[derive(Clone)]
pub(super) struct Web {
    pub hub: Hub,
    pub operator: Option<Arc<super::operator::Operator>>,
    pub addr: SocketAddr,
    pub config: Option<Arc<Config>>,
    pub snapshot: Shared,
    pub updates: watch::Sender<u64>,
    pub shutdown: watch::Sender<bool>,
    pub streams: Arc<Semaphore>,
    pub reads: Arc<Semaphore>,
}
impl Web {
    pub fn new(
        hub: Hub,
        addr: SocketAddr,
        config: Option<Arc<Config>>,
        snapshot: Shared,
        updates: watch::Sender<u64>,
        shutdown: watch::Sender<bool>,
    ) -> Self {
        Self {
            hub,
            operator: None,
            addr,
            config,
            snapshot,
            updates,
            shutdown,
            streams: Arc::new(Semaphore::new(20_100)),
            reads: Arc::new(Semaphore::new(16)),
        }
    }
}
pub(super) fn router(web: Web) -> Router {
    let mut core = im_service::transport::Web::new(web.hub.clone(), web.addr, web.shutdown.clone());
    core.operator = web.operator.clone();
    if let Some(config) = &web.config {
        core.scenarios = Arc::new(vec![
            Arc::new(im_service::Collaboration),
            Arc::new(super::research_scenario::Research(config.clone())),
        ]);
    }
    let application = Router::new()
        .route("/api/events", get(snapshots))
        .route("/api/snapshot", get(resource))
        .route("/api/people", get(resource).post(resource))
        .route("/api/people/{id}", get(resource))
        .route("/api/profile/{id}", get(resource))
        .route("/api/session/{id}", get(resource))
        .route("/api/evolution/lineage", get(resource))
        .route("/apps/research/", get(resource))
        .route("/apps/research/{*asset}", get(resource))
        .with_state(web);
    im_service::transport::router(core, application)
}
pub(super) use im_service::transport::{body, boundary, error, intent, json_response, response};
async fn snapshots(State(web): State<Web>, headers: HeaderMap) -> Response {
    if let Err(e) = boundary(&headers, web.addr) {
        return error(e);
    }
    let permit = match web.streams.clone().try_acquire_owned() {
        Ok(p) => p,
        Err(e) => return error(e),
    };
    let mut changes = web.updates.subscribe();
    let mut stop = web.shutdown.subscribe();
    let stream = async_stream::stream! {
        let _permit=permit;
        loop{
            if *stop.borrow(){break;}
            let value=web.snapshot.read().unwrap().clone();
            let event=Event::default().event("snapshot").id(value["revision"].to_string()).json_data(value).unwrap();
            yield Ok::<_,Infallible>(event);
            tokio::select!{_=changes.changed()=>{},_=stop.changed()=>break}
        }
    };
    Sse::new(stream)
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
        .into_response()
}
async fn resource(State(web): State<Web>, request: Request) -> Response {
    if let Err(e) = boundary(request.headers(), web.addr) {
        return error(e);
    }
    let permit = match web.reads.clone().try_acquire_owned() {
        Ok(p) => p,
        Err(e) => return error(format!("read backpressure: {e}")),
    };
    let path = request
        .uri()
        .path_and_query()
        .map(|v| v.as_str())
        .unwrap_or("/")
        .to_owned();
    if request.method() == "POST" && path == "/api/people" {
        if let Err(e) = intent(request.headers(), web.addr, "manage-people") {
            return error(e);
        }
        let bytes = match body(request, 16 * 1024).await {
            Ok(v) => v,
            Err(e) => return error(e),
        };
        return match tokio::task::spawn_blocking(move || {
            let _permit = permit;
            super::manage::apply(
                web.config.as_deref().context("identity unavailable")?,
                &bytes,
            )
        })
        .await
        {
            Ok(result) => json_response(result),
            Err(e) => error(e),
        };
    }
    if request.method() != "GET" {
        return response(StatusCode::METHOD_NOT_ALLOWED, "text/plain", "GET required");
    }
    match tokio::task::spawn_blocking(move || {
        let _permit = permit;
        super::resources::get(&path, &web)
    })
    .await
    {
        Ok(result) => result,
        Err(e) => error(e),
    }
}
