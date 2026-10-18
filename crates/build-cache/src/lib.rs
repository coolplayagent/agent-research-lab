//! Dependency cache copies are a performance input, never evaluation evidence.
//! Each task owns its writable copy. Only checksum-addressed archives and the
//! public crates.io sparse index are copied; credentials, configuration, locks,
//! extracted repositories and compiler output are never imported.
mod bazel;
mod cargo;
mod copying;
mod files;
mod metadata;
mod preparation;
mod python_toolchain;
mod report;
mod rust_toolchain;

const MAX_FILES: usize = 4096;
const MAX_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_FILE_BYTES: u64 = 512 * 1024 * 1024;
const REGISTRY: &str = "registry+https://github.com/rust-lang/crates.io-index";

pub use preparation::{applicable, prepare, ready};
pub use report::{PreparationDeferred, SeedReport, deferred};

#[cfg(test)]
mod test_support;
