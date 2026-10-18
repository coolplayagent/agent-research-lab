//! Explicit delivery adapters. No network or installation happens merely by loading a request.
mod evidence;
mod github;
mod host;
mod installation;
mod merge;
mod merge_gate;
mod package;
mod protocol;
mod pull_requests;
mod validation;

pub use evidence::{ReleaseEvidence, VerificationArtifact};
pub use host::{capture_text, unique_id};
pub use installation::{install_candidate, rollback_install};
pub use merge::merge_pull_request;
pub use merge_gate::evaluate_merge_gate;
pub use package::{file_sha256, package_sha256};
pub use protocol::{
    InstallReceipt, InstallRequest, MergeRequest, MergeSnapshot, PullRequestReceipt,
    PullRequestRequest, ReviewRecord,
};
pub use pull_requests::ensure_pull_request;
pub use validation::{relative_path, validate_commit, validate_digest, validate_identifier};

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
