//! Host-only task management; workers cannot call coordinator transitions.
use crate::{transport::*, *};
use axum::{
    extract::{Query, Request, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use collaboration_model::*;
use serde::Deserialize;
pub(super) fn response(result: Result<Value>) -> Response {
    match result {
        Ok(v) => axum::Json(v).into_response(),
        Err(e) => {
            let code = TaskErrorCode::ALL
                .iter()
                .copied()
                .find(|c| e.to_string().contains(c.as_str()))
                .unwrap_or(TaskErrorCode::Invalid);
            let status = match code {
                TaskErrorCode::NotFound => StatusCode::NOT_FOUND,
                TaskErrorCode::Forbidden => StatusCode::FORBIDDEN,
                TaskErrorCode::Conflict | TaskErrorCode::StaleAttempt => StatusCode::CONFLICT,
                TaskErrorCode::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
                _ => StatusCode::BAD_REQUEST,
            };
            (
                status,
                axum::Json(json!({"error":{"code":code,"parameters":{}},"code":code})),
            )
                .into_response()
        }
    }
}
#[derive(Default, Deserialize)]
pub struct Page {
    #[serde(default)]
    id: String,
    #[serde(default)]
    group_id: String,
    #[serde(default)]
    after: String,
}
pub async fn list(State(web): State<Web>, Query(page): Query<Page>) -> Response {
    response(
        async {
            if page.id.is_empty() {
                ensure!(!page.group_id.is_empty(), "{}", TaskErrorCode::Invalid);
                web.hub
                    .tasks(TaskCommand::List {
                        group_id: page.group_id,
                        after: page.after,
                        active: false,
                        questions: false,
                    })
                    .await
            } else {
                let value = web.hub.tasks(TaskCommand::Get { id: page.id }).await?;
                ensure!(
                    page.group_id.is_empty()
                        || value["input"]["group_id"].as_str() == Some(page.group_id.as_str()),
                    "{}",
                    TaskErrorCode::Forbidden
                );
                Ok(value)
            }
        }
        .await,
    )
}
pub async fn manage(State(web): State<Web>, request: Request) -> Response {
    response(async {
        intent(request.headers(),web.addr,"manage-crystal")?;
        let command:TaskCommand=serde_json::from_slice(&body(request,32768).await?)?;
        ensure!(matches!(&command,TaskCommand::Create{..}) || matches!(&command,TaskCommand::Change{action,..} if action.operator_allowed()),"{}",TaskErrorCode::Forbidden);
        web.hub.tasks(command).await
    }.await)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Question {
    message: Publish,
    people: Vec<String>,
}
pub async fn question(State(web): State<Web>, request: Request) -> Response {
    response(
        async {
            intent(request.headers(), web.addr, "manage-crystal")?;
            let q: Question = serde_json::from_slice(&body(request, 32768).await?)?;
            let input = TaskInput {
                id: format!(
                    "question-{}",
                    storage::digest(
                        format!("{}:{}", q.message.group_id, q.message.request_id).as_bytes()
                    )
                ),
                group_id: q.message.group_id.clone(),
                intent: q.message.text.clone(),
                budget: TaskBudget::default(),
            };
            web.hub
                .tasks(TaskCommand::Question {
                    input,
                    people: q.people,
                    message: q.message,
                })
                .await
        }
        .await,
    )
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CoordinatorSettings {
    revision: u64,
    adapter_id: String,
    model: Option<String>,
}
pub async fn coordinator(State(web): State<Web>) -> Response {
    response(
        async {
            let registry = web.services.as_ref().context(TaskErrorCode::Unavailable)?;
            let config = registry.coordinator_configuration()?;
            Ok(json!({"revision":config.revision,"coordinator":config.coordinator}))
        }
        .await,
    )
}
pub async fn configure(State(web): State<Web>, request: Request) -> Response {
    response(
        async {
            intent(request.headers(), web.addr, "manage-services")?;
            let input: CoordinatorSettings = serde_json::from_slice(&body(request, 4096).await?)?;
            let registry = web.services.as_ref().context(TaskErrorCode::Unavailable)?;
            let mut config = registry.configuration();
            ensure!(
                config.revision == input.revision,
                "{}",
                TaskErrorCode::Conflict
            );
            let adapter = config
                .adapters
                .iter()
                .find(|a| a.id == input.adapter_id && a.enabled)
                .context(TaskErrorCode::Unavailable)?;
            config.coordinator = Some(service_api::CoordinatorConfiguration {
                adapter_id: input.adapter_id,
                model: input.model,
                adapter_digest: storage::digest(&serde_json::to_vec(adapter)?),
                protocol_evidence: None,
            });
            service_registry::Registry::coordinator_adapter(&config)?;
            let config = registry.save(config)?;
            Ok(json!({"revision":config.revision,"coordinator":config.coordinator}))
        }
        .await,
    )
}

#[cfg(test)]
#[path = "tasks_tests.rs"]
mod tests;

pub async fn questions(State(web): State<Web>, Query(page): Query<Page>) -> Response {
    response(
        async {
            ensure!(!page.group_id.is_empty(), "{}", TaskErrorCode::Invalid);
            web.hub
                .tasks(TaskCommand::List {
                    group_id: page.group_id,
                    after: page.after,
                    active: false,
                    questions: true,
                })
                .await
        }
        .await,
    )
}

pub async fn test_coordinator(State(web): State<Web>, headers: HeaderMap) -> Response {
    response(
        async {
            intent(&headers, web.addr, "manage-services")?;
            let registry = web.services.as_ref().context(TaskErrorCode::Unavailable)?;
            let config = tokio::time::timeout(
                Duration::from_secs(90),
                crate::task_execution::probe(registry, true),
            )
            .await
            .context(TaskErrorCode::Unavailable)??;
            Ok(json!({"revision":config.revision,"coordinator":config.coordinator}))
        }
        .await,
    )
}
