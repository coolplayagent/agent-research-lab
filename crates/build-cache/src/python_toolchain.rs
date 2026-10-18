//! Select the literal rules_python default version for the exact host triple.
use crate::metadata::{read_tar_member, safe_component, sha256, verified_archive};
use anyhow::{Context, Result, ensure};
use std::{collections::BTreeSet, path::Path};

pub(super) fn python_toolchain_hashes(
    worktree: &Path,
    cas: &Path,
    metadata: &serde_json::Value,
    hashes: &mut BTreeSet<String>,
) -> Result<()> {
    let Some(archive) = verified_archive(cas, metadata)? else {
        return Ok(());
    };
    let prefix = metadata
        .get("strip_prefix")
        .and_then(serde_json::Value::as_str)
        .context("rules_python archive prefix missing")?;
    ensure!(
        safe_component(prefix),
        "unsupported rules_python archive prefix"
    );
    let module = read_tar_member(worktree, &archive, &format!("{prefix}/MODULE.bazel"))?;
    let versions = read_tar_member(worktree, &archive, &format!("{prefix}/python/versions.bzl"))?;
    let triple = format!("{}-unknown-linux-gnu", std::env::consts::ARCH);
    hashes.insert(python_default_hash(&module, &versions, &triple)?);
    Ok(())
}

fn python_default_hash(module: &str, versions: &str, triple: &str) -> Result<String> {
    let default = module
        .split_once("python.defaults(")
        .and_then(|(_, value)| value.split_once(')'))
        .context("rules_python default missing")?
        .0;
    let minor = default
        .lines()
        .filter_map(|line| line.split_once('='))
        .find(|(key, _)| key.trim() == "python_version")
        .context("rules_python default version missing")?
        .1
        .trim()
        .trim_end_matches(',')
        .trim_matches('"');
    let mapping = versions
        .split_once("MINOR_MAPPING = {")
        .and_then(|(_, value)| value.split_once("\n}"))
        .context("Python minor mapping missing")?
        .0;
    let version =
        literal_string_entry(mapping, minor).context("Python minor version not literal")?;
    let block = versions
        .split_once(&format!("    \"{version}\": {{"))
        .and_then(|(_, value)| value.split_once("\n    },"))
        .context("Python runtime metadata missing")?
        .0;
    let digest = literal_string_entry(block, triple).context("Python host runtime hash missing")?;
    ensure!(sha256(digest), "invalid Python runtime hash");
    Ok(digest.to_owned())
}

fn literal_string_entry<'a>(metadata: &'a str, key: &str) -> Option<&'a str> {
    metadata
        .lines()
        .filter_map(|line| line.trim().split_once(':'))
        .find(|(name, _)| name.trim() == format!("\"{key}\""))?
        .1
        .trim()
        .trim_end_matches(',')
        .strip_prefix('"')?
        .strip_suffix('"')
}

#[cfg(test)]
#[path = "python_toolchain_tests.rs"]
mod tests;
