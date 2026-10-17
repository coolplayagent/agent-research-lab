//! Application service composition and compatibility exports for host integrations.
mod cli;
use research_console as dashboard;
pub mod service;
use software_delivery as software_activation;

pub use agent_backend;
pub use agent_policy;
pub use automation;
pub use budget;
pub use build_cache;
pub use collaboration_experiment;
pub use communication;
pub use config;
pub use delivery;
pub use evolution;
pub use evolution_cli;
pub use freshness;
pub use inputs;
pub use isolation;
pub use knowledge;
pub use lease_heartbeat;
pub use multi_agent;
pub use process;
pub use runtime;
pub use storage;
pub use targets;
pub use task;
pub use workflow;
