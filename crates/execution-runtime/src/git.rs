//! Bounded Git observations through the shared process adapter.
use anyhow::Result;
use std::path::Path;

pub(super) fn git(cwd: &Path, args: &[&str]) -> Result<String> {
    process::checked(
        "git",
        &args.iter().map(|s| (*s).to_string()).collect::<Vec<_>>(),
        cwd,
    )
}
