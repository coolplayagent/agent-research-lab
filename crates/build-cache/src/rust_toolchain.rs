//! Select only literal Rust bootstrap archives and pinned host toolchain components.
use crate::{
    MAX_FILES,
    files::read_bounded,
    metadata::{read_tar_member, sha256, verified_archive},
};
use anyhow::{Context, Result, ensure};
use std::{collections::BTreeSet, path::Path};

pub(super) fn rust_toolchain_hashes(
    worktree: &Path,
    cas: &Path,
    metadata: &serde_json::Value,
    hashes: &mut BTreeSet<String>,
) -> Result<()> {
    let Some(archive) = verified_archive(cas, metadata)? else {
        return Ok(());
    };
    let member = |name: &str| read_tar_member(worktree, &archive, &format!("./{name}"));
    // These are the literal archive declarations of rules_rust's non-dev
    // internal extensions. Read only these named metadata files from the
    // checksum-verified distribution; never run candidate Starlark on the host.
    for path in [
        "rust/private/repository_utils.bzl",
        "cargo/3rdparty/crates/crates.bzl",
    ] {
        literal_archive_hashes(&member(path)?, hashes)?;
    }
    let common = member("rust/private/common.bzl")?;
    let constant = |name: &str| -> Result<String> {
        let value = common
            .lines()
            .filter_map(|line| line.split_once('='))
            .find(|(key, _)| key.trim() == name)
            .context("rules_rust default missing")?
            .1
            .trim();
        Ok(value
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .context("rules_rust default must be a literal")?
            .to_owned())
    };
    let mut versions = BTreeSet::from([constant("DEFAULT_RUST_VERSION")?]);
    let nightly = constant("DEFAULT_NIGHTLY_ISO_DATE")?;
    let config = worktree.join("rust-toolchain.toml");
    if config.exists() {
        let config: toml::Value =
            toml::from_str(std::str::from_utf8(&read_bounded(&config, 1024 * 1024)?)?)?;
        if let Some(version) = config
            .get("toolchain")
            .and_then(|v| v.get("channel"))
            .and_then(toml::Value::as_str)
        {
            ensure!(
                !version.is_empty() && version.bytes().all(|b| b.is_ascii_digit() || b == b'.'),
                "cache reuse requires a pinned Rust version"
            );
            let module =
                String::from_utf8(read_bounded(&worktree.join("MODULE.bazel"), 1024 * 1024)?)?;
            let module: String = module.chars().filter(|c| !c.is_whitespace()).collect();
            if module.contains(&format!("versions=[\"{version}\"]")) {
                versions.insert(version.to_owned());
            }
        }
    }
    let triple = format!("{}-unknown-linux-gnu", std::env::consts::ARCH);
    let tools = ["rustc", "rust-std", "cargo", "clippy", "llvm-tools"];
    let known = member("rust/private/known_shas.bzl")?;
    for line in known.lines() {
        let fields: Vec<_> = line.split('"').collect();
        if fields.len() < 4 || !sha256(fields[3]) {
            continue;
        }
        let name = fields[1];
        let stable = versions.iter().any(|version| {
            tools
                .iter()
                .any(|tool| name == format!("{tool}-{version}-{triple}.tar.xz"))
        });
        let formatter = name == format!("{nightly}/rustfmt-nightly-{triple}.tar.xz");
        if stable || formatter {
            hashes.insert(fields[3].to_owned());
        }
    }
    Ok(())
}

fn literal_archive_hashes(metadata: &str, hashes: &mut BTreeSet<String>) -> Result<()> {
    for line in metadata.lines() {
        let Some((key, value)) = line.trim().split_once('=') else {
            continue;
        };
        if key.trim() != "sha256" {
            continue;
        }
        // Computed expressions are not dependency evidence. Exact literals only.
        if let Some(digest) = value
            .trim()
            .trim_end_matches(',')
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
            .filter(|value| sha256(value))
        {
            hashes.insert(digest.to_owned());
        }
    }
    ensure!(
        hashes.len() <= MAX_FILES,
        "too many rules_rust dependency hashes"
    );
    Ok(())
}

#[cfg(test)]
#[path = "rust_toolchain_tests.rs"]
mod tests;
