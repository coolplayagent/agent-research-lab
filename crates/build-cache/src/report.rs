//! Stable cache preparation metrics and resumable-deadline diagnostic.
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SeedReport {
    pub(super) schema_version: u32,
    pub(super) inputs_digest: String,
    pub cargo_archives: usize,
    pub cargo_index_entries: usize,
    pub bazel_archives: usize,
    pub bazel_binaries: usize,
    pub missing_cargo_archives: Vec<String>,
    pub copied_files: usize,
    pub copied_bytes: u64,
    pub reused_files: usize,
    pub retained_files: usize,
    pub retained_bytes: u64,
    pub missing_bazel_hashes: Vec<String>,
    pub elapsed_millis: u64,
}

#[derive(Debug)]
pub struct PreparationDeferred;
impl std::fmt::Display for PreparationDeferred {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("build cache preparation deadline exhausted; private copies can be resumed")
    }
}
impl std::error::Error for PreparationDeferred {}

pub fn deferred(error: &anyhow::Error) -> bool {
    error.downcast_ref::<PreparationDeferred>().is_some()
}
