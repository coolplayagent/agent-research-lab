//! Host-controlled freshness gates. A network or validation failure never permits
//! an older local checkout, skill installation, or release identity as a fallback.
mod client;
mod files;
mod identity;
mod protocol;
mod releases;
mod remote;
mod skills;
mod source_gate;

pub use client::RemoteClient;
pub use files::skill_tree_digest;
pub use protocol::{RemoteSnapshot, SkillSnapshot, SkillsManifest, SourceQualitygate};
pub use remote::{remote_snapshot, verify_fresh, verify_remote_binding};
pub use skills::{verify_installed_bytes, verify_skills};

#[cfg(test)]
mod test_support;
