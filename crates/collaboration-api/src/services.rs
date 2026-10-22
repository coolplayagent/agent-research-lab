//! Operator-owned service configuration; no credentials are returned by this API.
use crate::{transport::*, *};
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::response::Response;
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CleanupPeople {
    revision: u64,
}
pub async fn view(State(web): State<Web>) -> Response {
    settings_response(
        async {
            web.services
                .as_ref()
                .context("service management is unavailable")?
                .view()
                .await
        }
        .await,
    )
}
pub async fn save(State(web): State<Web>, request: Request) -> Response {
    settings_response(
        async {
            intent(request.headers(), web.addr, "manage-services")?;
            let next: service_api::Configuration =
                serde_json::from_slice(&body(request, 256 * 1024).await?)?;
            for group in next.knowledge_groups.keys() {
                web.hub.group(group).await?;
            }
            for goal in next.knowledge_goals.keys() {
                web.hub.goal(goal).await?;
            }
            for person in next.bindings.keys() {
                web.hub.person(person).await?;
            }
            Ok(json!(
                web.services
                    .as_ref()
                    .context("service management is unavailable")?
                    .save_settings(next)?
            ))
        }
        .await,
    )
}
pub async fn cleanup_people(State(web): State<Web>, request: Request) -> Response {
    settings_response(
        async {
            intent(request.headers(), web.addr, "manage-services")?;
            let input: CleanupPeople = serde_json::from_slice(&body(request, 4096).await?)?;
            let config = web
                .services
                .as_ref()
                .context("service management is unavailable")?
                .configuration();
            ensure!(
                config.revision == input.revision,
                service_api::ConfigurationFailure(
                    contracts::ServiceConfigurationErrorCode::RevisionConflict,
                )
            );
            let retained = config.bindings.keys().cloned().collect();
            let archived = web.hub.archive_unbound(retained).await?;
            Ok(json!({"archived":archived,"configuration_revision":input.revision}))
        }
        .await,
    )
}

pub async fn prepare(State(web): State<Web>, request: Request) -> Response {
    settings_response(
        async {
            intent(request.headers(), web.addr, "manage-services")?;
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Prepare {
                revision: u64,
            }
            let input: Prepare = serde_json::from_slice(&body(request, 1024).await?)?;
            let registry = web
                .services
                .as_ref()
                .context("service management is unavailable")?;
            ensure!(
                input.revision == registry.configuration().revision,
                service_api::ConfigurationFailure(
                    contracts::ServiceConfigurationErrorCode::RevisionConflict
                )
            );
            registry.prepare_local_execution().await.map_err(|e| {
                e.context(service_api::ConfigurationFailure(
                    contracts::ServiceConfigurationErrorCode::RuntimeUnavailable,
                ))
            })?;
            registry.view().await
        }
        .await,
    )
}
fn settings_response(result: Result<serde_json::Value>) -> Response {
    use contracts::ServiceConfigurationErrorCode as Code;
    match result {
        Ok(value) => json_response(Ok(value)),
        Err(error) => {
            if let Some(diagnostic) = service_protocol::knowledge_error(&error) {
                return response(
                    StatusCode::BAD_REQUEST,
                    "application/json",
                    serde_json::to_vec(&json!({"error":diagnostic})).unwrap(),
                );
            }
            let code = error
                .downcast_ref::<service_api::ConfigurationFailure>()
                .map(|e| e.0)
                .unwrap_or_else(|| match service_registry::binding_failure_code(&error) {
                    contracts::BindingErrorCode::RevisionConflict => Code::RevisionConflict,
                    contracts::BindingErrorCode::ValidationRequired => Code::ValidationRequired,
                    _ => Code::Invalid,
                });
            let status = match code {
                Code::RevisionConflict => StatusCode::CONFLICT,
                Code::RuntimeUnavailable => StatusCode::SERVICE_UNAVAILABLE,
                _ => StatusCode::BAD_REQUEST,
            };
            response(
                status,
                "application/json",
                serde_json::to_vec(&json!({"code":code,"error":{"code":code,"parameters":{}}}))
                    .unwrap(),
            )
        }
    }
}
#[cfg(test)]
#[path = "services_tests.rs"]
mod tests;
