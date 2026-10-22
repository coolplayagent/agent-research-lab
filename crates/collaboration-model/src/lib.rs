//! Storage-free collaboration data contracts and deterministic validation.
mod deletion;
pub use deletion::*;
mod model;
pub use model::*;
mod goal_model;
pub use goal_model::*;
mod coordination_model;
pub use coordination_model::*;
mod outcome_model;
pub use outcome_model::*;

/// Stable content digest shared across protocol and persistence boundaries.
pub fn digest(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}

/// Shared scenario protocol for ordinary collaboration and optional adapters.
pub const SCENARIO_PROTOCOL_VERSION: u32 = 1;

mod task_model;
pub use task_model::*;
mod task_machine;
