//! Vendor bridges retain a single versioned protocol at the execution boundary.
mod discovery;
mod dsh_config;
mod process;
mod providers;
mod snapshots;
pub use discovery::{discover, discovery_failure_code, executable_digest, rediscover};
pub use dsh_config::selected_model;
pub use providers::{Options, execute};
pub use snapshots::{configuration_digest, environment_names, prepare_home, runtime_paths};
