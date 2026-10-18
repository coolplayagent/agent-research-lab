//! Shared syntax constraints for immutable identities and confined paths.
use anyhow::{Context, Result, ensure};
use std::path::{Component, Path};

pub fn validate_digest(value: &str) -> Result<()> {
    ensure!(
        value.len() == 64 && value.bytes().all(|c| c.is_ascii_hexdigit()),
        "expected a full SHA-256 digest"
    );
    Ok(())
}

pub fn validate_commit(value: &str) -> Result<()> {
    ensure!(
        (value.len() == 40 || value.len() == 64) && value.bytes().all(|c| c.is_ascii_hexdigit()),
        "expected a full immutable Git commit"
    );
    Ok(())
}

pub fn validate_identifier(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.len() <= 100
            && !value.starts_with('.')
            && value
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_'),
        "invalid identifier"
    );
    Ok(())
}

pub fn relative_path(path: &Path) -> Result<()> {
    ensure!(
        !path.as_os_str().is_empty()
            && path.components().all(|p| matches!(p, Component::Normal(_))),
        "path must be confined and relative"
    );
    Ok(())
}

pub(super) fn validate_repository(repository: &str) -> Result<(&str, &str)> {
    let (owner, name) = repository
        .split_once('/')
        .context("repository must be OWNER/REPO")?;
    validate_identifier(owner)?;
    // GitHub repositories also permit dots.
    ensure!(
        !name.is_empty()
            && name
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c)),
        "invalid repository name"
    );
    Ok((owner, name))
}
