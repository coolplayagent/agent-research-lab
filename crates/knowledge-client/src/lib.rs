//! Typed RPC client shared by the operator console and CLI; no adapter or store dependency.
use anyhow::Result;
use std::{path::Path, time::Duration};
pub async fn request(
    endpoint: &Path,
    request: knowledge_model::Request,
) -> Result<serde_json::Value> {
    service_protocol::call(
        endpoint,
        &service_api::OutcomeServiceRequest::Knowledge { request },
        Duration::from_secs(60),
    )
    .await
}
