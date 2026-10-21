//! Operator proxy for external knowledge services. Configuration lives in the service that owns it.
use crate::{transport::*, *};
use axum::{
    extract::{Request, State},
    response::Response,
};
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    service_id: String,
    configuration_revision: u64,
    request: knowledge_model::Request,
}
pub async fn manage(State(web): State<Web>, request: Request) -> Response {
    json_response(
        async {
            intent(request.headers(), web.addr, "manage-knowledge")?;
            let input: Input = serde_json::from_slice(&body(request, 2 * 1024 * 1024).await?)?;
            let config = web
                .services
                .as_ref()
                .context("knowledge_service_unavailable")?
                .configuration();
            ensure!(
                config.revision == input.configuration_revision,
                "knowledge_configuration_conflict"
            );
            let service = config
                .outcome_services
                .iter()
                .find(|s| {
                    s.id == input.service_id
                        && s.kind == contracts::OutcomeServiceKind::Knowledge
                        && s.enabled
                })
                .context("knowledge_service_unavailable")?;
            web.services
                .as_ref()
                .unwrap()
                .prepare_knowledge(service)
                .await?;
            service_api::probe_outcome_service(service).await?;
            knowledge_client::request(&service.endpoint, input.request).await
        }
        .await,
    )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LocalInput {
    configuration_revision: u64,
    request: knowledge_model::Request,
}
pub async fn local(State(web): State<Web>, request: Request) -> Response {
    json_response(async {
        intent(request.headers(), web.addr, "manage-knowledge")?;
        let input: LocalInput = serde_json::from_slice(&body(request, 2 * 1024 * 1024).await?)?;
        ensure!(matches!(input.request, knowledge_model::Request::Inspect { .. } | knowledge_model::Request::Add { .. }), "knowledge_local_operation_required");
        let registry = web.services.as_ref().context("knowledge_service_unavailable")?;
        let service = registry.local_knowledge(input.configuration_revision).await?;
        let result = knowledge_client::request(&service.endpoint, input.request).await?;
        Ok(json!({"service_id":service.id,"configuration_revision":registry.configuration().revision,"result":result}))
    }.await)
}
