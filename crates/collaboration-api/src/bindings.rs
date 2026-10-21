//! Local operator binding operations. Every mutation retains the origin/intent gate.
use crate::{transport::*, *};
use axum::{
    extract::{Path as RoutePath, Request, State},
    response::Response,
};
pub async fn list(State(web): State<Web>) -> Response {
    json_response(
        web.services
            .as_ref()
            .context("service management is unavailable")
            .map(|r| json!(r.binding_operations())),
    )
}
pub async fn get(State(web): State<Web>, RoutePath(id): RoutePath<String>) -> Response {
    binding_response(
        web.services
            .as_ref()
            .context("service management is unavailable")
            .and_then(|r| Ok(json!(r.binding_operation(&id)?))),
    )
}
pub async fn start(State(web): State<Web>, request: Request) -> Response {
    binding_response(
        async {
            intent(request.headers(), web.addr, "manage-services")?;
            let input: service_api::BindingRequest =
                serde_json::from_slice(&body(request, 16 * 1024).await?)?;
            input.validate()?;
            web.hub.person(&input.person_id).await?;
            Ok(json!(
                web.services
                    .as_ref()
                    .context("service management is unavailable")?
                    .start_binding(input)?
            ))
        }
        .await,
    )
}
pub async fn cancel(
    State(web): State<Web>,
    RoutePath(id): RoutePath<String>,
    request: Request,
) -> Response {
    binding_response((|| {
        intent(request.headers(), web.addr, "manage-services")?;
        Ok(json!(
            web.services
                .as_ref()
                .context("service management is unavailable")?
                .cancel_binding(&id)?
        ))
    })())
}

fn binding_response(result: Result<serde_json::Value>) -> Response {
    match result {
        Ok(value) => json_response(Ok(value)),
        Err(error) => {
            let code = service_registry::binding_failure_code(&error);
            response(
                axum::http::StatusCode::BAD_REQUEST,
                "application/json",
                serde_json::to_vec(&json!({"error":{"code":code,"parameters":{"osCode":service_protocol::os_code(&error).map_or("-".into(), |code| code.to_string())}}})).unwrap(),
            )
        }
    }
}

#[cfg(test)]
#[path = "bindings_tests.rs"]
mod tests;
