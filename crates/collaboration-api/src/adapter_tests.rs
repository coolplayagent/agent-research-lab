//! Local operator adapter tests, independent of collaboration identities. Every mutation retains the origin/intent gate.
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
            .map(|r| json!(r.adapter_test_operations())),
    )
}
pub async fn get(State(web): State<Web>, RoutePath(id): RoutePath<String>) -> Response {
    adapter_test_response(
        web.services
            .as_ref()
            .context("service management is unavailable")
            .and_then(|r| Ok(json!(r.adapter_test_operation(&id)?))),
    )
}
pub async fn start(State(web): State<Web>, request: Request) -> Response {
    adapter_test_response(
        async {
            intent(request.headers(), web.addr, "manage-services")?;
            let input: service_api::AdapterTestRequest =
                serde_json::from_slice(&body(request, 16 * 1024).await?)?;
            input.validate()?;
            Ok(json!(
                web.services
                    .as_ref()
                    .context("service management is unavailable")?
                    .start_adapter_test(input)?
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
    adapter_test_response((|| {
        intent(request.headers(), web.addr, "manage-services")?;
        Ok(json!(
            web.services
                .as_ref()
                .context("service management is unavailable")?
                .cancel_adapter_test(&id)?
        ))
    })())
}

fn adapter_test_response(result: Result<serde_json::Value>) -> Response {
    match result {
        Ok(value) => json_response(Ok(value)),
        Err(error) => {
            let code = service_registry::adapter_test_failure_code(&error);
            response(
                axum::http::StatusCode::BAD_REQUEST,
                "application/json",
                serde_json::to_vec(&json!({"error":{"code":code,"parameters":{"osCode":service_protocol::os_code(&error).map_or("-".into(), |code| code.to_string())}}})).unwrap(),
            )
        }
    }
}

#[cfg(test)]
#[path = "adapter_tests_tests.rs"]
mod tests;
