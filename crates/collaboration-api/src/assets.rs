//! Built frontend modules; TypeScript is checked and regenerated through Bazel.
use crate::transport::response;
use axum::{http::StatusCode, response::Response};
pub(crate) fn contains(path: &str) -> bool {
    outcome_hub_web::get(path).is_some()
}
pub(crate) fn get(path: &str) -> Response {
    match outcome_hub_web::get(path) {
        Some((mime, body)) => response(StatusCode::OK, mime, body),
        None => response(
            StatusCode::NOT_FOUND,
            "application/json",
            br#"{"error":"Resource unavailable"}"#.as_slice(),
        ),
    }
}
