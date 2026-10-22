//! Research metadata is a core collaboration capability, independent of scenario execution.
use crate::transport::*;
use axum::{
    extract::{Query, Request, State},
    response::Response,
};
use serde::Deserialize;
#[derive(Default, Deserialize)]
pub struct Page {
    #[serde(default)]
    after: String,
    #[serde(default)]
    query: String,
    #[serde(default)]
    topic_id: String,
}
pub async fn topics(State(web): State<Web>, Query(page): Query<Page>) -> Response {
    json_response(web.hub.topics(&page.after, &page.query).await)
}
pub async fn subjects(State(web): State<Web>, Query(page): Query<Page>) -> Response {
    json_response(web.hub.subjects(&page.topic_id, &page.after).await)
}
pub async fn change(State(web): State<Web>, request: Request) -> Response {
    json_response(
        async {
            intent(request.headers(), web.addr, "manage-research")?;
            let change: collaboration_model::ResearchChange =
                serde_json::from_slice(&body(request, 32 * 1024).await?)?;
            web.hub.research_change(change).await
        }
        .await,
    )
}
