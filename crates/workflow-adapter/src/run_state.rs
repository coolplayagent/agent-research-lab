//! Parse external run observations into the shared state domain.
use contracts::WorkflowState;
use serde_json::Value;

/// Preserve historical observation aliases without extending the canonical wire enum.
/// This only classifies a state; it does not verify run identity or committed results.
pub fn parse_run_state(value: &Value) -> Option<WorkflowState> {
    match value.as_str() {
        Some("canceled") => Some(WorkflowState::Cancelled),
        Some("timed_out") => Some(WorkflowState::Failed),
        _ => WorkflowState::from_value(value),
    }
}

#[cfg(test)]
#[path = "run_state_tests.rs"]
mod tests;
