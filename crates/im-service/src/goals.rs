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
    #[serde(default)]
    current: bool,
}
pub async fn list(State(web): State<Web>, Query(page): Query<Page>) -> Response {
    json_response(
        async {
            if page.current {
                anyhow::ensure!(
                    page.after.is_empty(),
                    "current goal does not support a cursor"
                );
                web.hub.current_goal(&page.group_id).await
            } else {
                web.hub.goals(&page.group_id, "", &page.after, false).await
            }
        }
        .await,
    )
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
                let scenarios =
                    crate::scenarios::configured(&web.scenarios, web.services.as_deref())?;
                let adapter = scenarios
                    .iter()
                    .find(|s| s.descriptor().id == goal.scenario)
                    .cloned()
                    .context("scenario unavailable")?;
                let input = goal.clone();
                let checked = adapter.clone();
                tokio::task::spawn_blocking(move || checked.validate(&input)).await??;
                adapter.admission(goal).await?;
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
    json_response(async {
        let scenarios = crate::scenarios::configured(&web.scenarios, web.services.as_deref())?;
        let mut status = scenarios.iter().map(|scenario| (scenario.descriptor().id, scenario.status())).collect::<std::collections::BTreeMap<_,_>>();
        if let Some(registry) = &web.services {
            let mut probes = tokio::task::JoinSet::new();
            for registration in registry.configuration().scenarios.into_iter().filter(|registration| registration.enabled) {
                probes.spawn(async move {
                    let result = service_api::probe_scenario(&registration).await;
                    let healthy = result.is_ok();
                    (registration.id, json!({"available":healthy,"configured":true,"status":if healthy { contracts::ScenarioStatus::Ready } else { contracts::ScenarioStatus::Unavailable },"health":if healthy { contracts::ServiceHealth::Healthy } else { contracts::ServiceHealth::Unavailable },"error":result.err().map(|error|error.to_string())}))
                });
            }
            while let Some(result) = probes.join_next().await { let (id, value) = result?; status.insert(id, value); }
        }
        Ok(json!({"scenarios":scenarios.iter().map(|scenario|scenario.descriptor()).collect::<Vec<_>>(),"status":status}))
    }.await)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Discussion {
    goal: crystal::GoalInput,
    message: crystal::Publish,
}
pub async fn discuss(State(web): State<Web>, request: Request) -> Response {
    json_response(
        async {
            intent(request.headers(), web.addr, "manage-crystal")?;
            let _permit = web
                .reads
                .clone()
                .try_acquire_owned()
                .context("discussion backpressure")?;
            let mut input: Discussion = serde_json::from_slice(&body(request, 64 * 1024).await?)?;
            input.goal.id = format!(
                "mention-{}",
                storage::digest(
                    format!("{}:{}", input.message.group_id, input.message.request_id).as_bytes()
                )
            );
            input.goal.validate()?;
            let scenarios = crate::scenarios::configured(&web.scenarios, web.services.as_deref())?;
            let adapter = scenarios
                .iter()
                .find(|s| s.descriptor().id == input.goal.scenario)
                .cloned()
                .context("scenario unavailable")?;
            let goal = input.goal.clone();
            let checked = adapter.clone();
            tokio::task::spawn_blocking(move || checked.validate(&goal)).await??;
            adapter.admission(&input.goal).await?;
            web.hub.discuss(input.goal, input.message).await
        }
        .await,
    )
}
