//! Agent grants see only their own memberships and message history.
use crate::{transport::*, *};
use axum::{
    extract::{Query, State},
    http::HeaderMap,
    response::Response,
};
use serde::Deserialize;
#[derive(Deserialize, Default)]
pub struct Page {
    #[serde(default)]
    after: String,
    #[serde(default)]
    query: String,
    #[serde(default)]
    group_id: String,
    #[serde(default)]
    sequence: u64,
}
pub async fn people(State(web): State<Web>, Query(page): Query<Page>) -> Response {
    json_response(web.hub.people(&page.after, &page.query, 100).await)
}
pub async fn inbox(
    State(web): State<Web>,
    headers: HeaderMap,
    Query(page): Query<Page>,
) -> Response {
    json_response(async { web.hub.inbox(token(&headers)?, &page.after, 100).await }.await)
}
pub async fn history(
    State(web): State<Web>,
    headers: HeaderMap,
    Query(page): Query<Page>,
) -> Response {
    json_response(
        async {
            let actor = web.hub.authenticate(token(&headers)?).await?;
            web.hub.authorize(&actor, &page.group_id).await?;
            let messages = web
                .hub
                .history(&page.group_id, page.sequence, crystal::PAGE_SIZE)
                .await?;
            web.hub.authenticate(token(&headers)?).await?;
            web.hub.authorize(&actor, &page.group_id).await?;
            Ok(json!({"group":web.hub.group(&page.group_id).await?,"messages":messages}))
        }
        .await,
    )
}
