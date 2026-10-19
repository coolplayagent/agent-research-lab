//! Native Windows isolation primitives shared by the service transports and runner.
#[cfg(windows)]
mod security;
#[cfg(windows)]
pub use security::*;
#[cfg(windows)]
mod container;
#[cfg(windows)]
mod job;
#[cfg(windows)]
mod launch;
#[cfg(windows)]
mod pipes;
#[cfg(windows)]
pub use container::Container;
#[cfg(windows)]
pub use job::*;
#[cfg(windows)]
pub use launch::Process;
#[cfg(windows)]
pub use pipes::spawn_piped;

pub fn endpoint_digest(value: &str) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
