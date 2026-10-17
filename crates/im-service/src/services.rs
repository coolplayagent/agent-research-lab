//! Operator-owned service configuration; no credentials are returned by this API.
use crate::{transport::*, *};
use axum::extract::{Request, State};
use axum::response::Response;
pub async fn view(State(web): State<Web>) -> Response {
    json_response(
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
    json_response(
        async {
            intent(request.headers(), web.addr, "manage-services")?;
            let next: service_api::Configuration =
                serde_json::from_slice(&body(request, 256 * 1024).await?)?;
            for person in next.bindings.keys() {
                web.hub.person(person).await?;
            }
            for topic_id in next.outcome_bindings.keys() {
                web.hub
                    .outcomes(
                        None,
                        collaboration_model::OutcomeCommand::Paths {
                            topic_id: topic_id.clone(),
                            after: String::new(),
                        },
                    )
                    .await?;
            }
            for registration in &next.scenarios {
                ensure!(
                    !web.scenarios
                        .iter()
                        .any(|scenario| scenario.descriptor().id == registration.id),
                    "registered scenario conflicts with an installed adapter"
                );
            }
            Ok(json!(
                web.services
                    .as_ref()
                    .context("service management is unavailable")?
                    .save(next)?
            ))
        }
        .await,
    )
}
