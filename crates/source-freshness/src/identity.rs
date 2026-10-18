//! Authorized repository identities, immutable digests and branch encoding.
use anyhow::{Context, Result, ensure};

pub(super) fn github_origin(origin: &str) -> Result<String> {
    let repository = origin
        .strip_prefix("https://github.com/")
        .or_else(|| origin.strip_prefix("git@github.com:"))
        .or_else(|| origin.strip_prefix("ssh://git@github.com/"))
        .context("origin must identify an authorized GitHub repository")?;
    let repository = repository
        .trim_end_matches('/')
        .strip_suffix(".git")
        .unwrap_or(repository.trim_end_matches('/'));
    trusted_repository(repository)?;
    Ok(repository.to_owned())
}

pub(super) fn trusted_repository(repository: &str) -> Result<()> {
    let parts: Vec<_> = repository.split('/').collect();
    ensure!(
        parts.len() == 2
            && parts.iter().all(|s| !s.is_empty()
                && *s != "."
                && *s != ".."
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')),
        "invalid GitHub repository identity"
    );
    ensure!(
        parts[0].eq_ignore_ascii_case("coolplayagent")
            || repository.eq_ignore_ascii_case("stevetdp/superpod"),
        "repository is outside the authorized namespace"
    );
    Ok(())
}

pub(super) fn validate_hex(value: &str, length: usize) -> Result<()> {
    ensure!(
        value.len() == length
            && value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "invalid immutable digest"
    );
    Ok(())
}

pub(super) fn validate_branch(branch: &str) -> Result<()> {
    ensure!(
        !branch.is_empty()
            && !branch.starts_with('-')
            && !branch.starts_with('/')
            && !branch.ends_with('/')
            && !branch.contains("..")
            && !branch.contains("@{")
            && !branch.contains("//")
            && !branch.ends_with('.')
            && !branch.ends_with(".lock")
            && !branch
                .chars()
                .any(|c| c.is_control() || " ~^:?*[\\".contains(c)),
        "invalid default branch name"
    );
    Ok(())
}

pub(super) fn url_component(text: &str) -> String {
    text.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

#[cfg(test)]
#[path = "identity_tests.rs"]
mod tests;
