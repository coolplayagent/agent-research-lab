use crate::{transport::*, *};
use axum::{
    extract::{Query, Request, State},
    http::HeaderMap,
    response::Response,
};
use serde::Deserialize;
#[derive(Default, Deserialize)]
pub struct Page {
    #[serde(default)]
    group_id: String,
    #[serde(default)]
    after: String,
}
pub async fn list(State(web): State<Web>, Query(page): Query<Page>) -> Response {
    json_response(web.hub.goals(&page.group_id, "", &page.after, false).await)
}
pub async fn manage(State(web): State<Web>, request: Request) -> Response {
    json_response(
        async {
            intent(request.headers(), web.addr, "manage-crystal")?;
            let _permit = web
                .reads
                .clone()
                .try_acquire_owned()
                .context("goal management backpressure")?;
            let change: crystal::GoalChange =
                serde_json::from_slice(&body(request, 192 * 1024).await?)?;
            if let crystal::GoalChange::Create { goal } = &change {
                goal.validate()?;
                let adapter = web
                    .scenarios
                    .iter()
                    .find(|s| s.descriptor().id == goal.scenario)
                    .cloned()
                    .context("scenario unavailable")?;
                let input = goal.clone();
                tokio::task::spawn_blocking(move || adapter.validate(&input)).await??;
            }
            Ok(json!(web.hub.change_goal(change).await?))
        }
        .await,
    )
}
pub async fn assigned(
    State(web): State<Web>,
    headers: HeaderMap,
    Query(page): Query<Page>,
) -> Response {
    json_response(async { web.hub.assigned_goals(token(&headers)?, &page.after).await }.await)
}
pub async fn work(State(web): State<Web>, request: Request) -> Response {
    json_response(
        async {
            let credential = token(request.headers())?.to_owned();
            let change = serde_json::from_slice(&body(request, 32 * 1024).await?)?;
            Ok(json!(web.hub.work(&credential, change).await?))
        }
        .await,
    )
}
pub async fn scenarios(State(web): State<Web>) -> Response {
    json_response(Ok(
        json!({"scenarios": web.scenarios.iter().map(|s| s.descriptor()).collect::<Vec<_>>(), "status": web.scenarios.iter().map(|s| (s.descriptor().id, s.status())).collect::<std::collections::BTreeMap<_,_>>()}),
    ))
}
