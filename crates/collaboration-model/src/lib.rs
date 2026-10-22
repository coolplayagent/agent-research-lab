//! Storage-free collaboration data contracts and deterministic validation.
mod deletion;
pub use deletion::*;
mod model;
pub use model::*;
mod goal_model;
pub use goal_model::*;
mod coordination_model;
pub use coordination_model::*;
mod iteration_model;
pub use iteration_model::*;
mod outcome_model;
pub use outcome_model::*;

/// Stable content digest shared across protocol and persistence boundaries.
pub fn digest(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}
