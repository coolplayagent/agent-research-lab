//! Verify citations against regular Git blobs at frozen repository revisions.
use crate::protocol::{Citation, Plan};
use anyhow::{Context, Result, ensure};
use std::{
    path::{Component, Path},
    time::Duration,
};

pub(super) fn check_citation(plan: &Plan, citation: &Citation) -> Result<()> {
    let repository = plan
        .inputs
        .repositories
        .get(&citation.repository)
        .context("citation repository not bound to pilot")?;
    ensure!(
        citation.commit == repository.upstream.commit,
        "citation uses another commit"
    );
    safe_source_path(&citation.path)?;
    let path = Path::new(&citation.path);
    ensure!(
        !citation.path.is_empty()
            && !path.is_absolute()
            && path.components().all(|c| matches!(c, Component::Normal(_))),
        "citation path escapes repository"
    );
    ensure!(
        citation.start_line > 0
            && citation.end_line >= citation.start_line
            && citation.end_line - citation.start_line <= 200,
        "invalid citation line range"
    );
    regular_source(&repository.checkout, &citation.commit, &citation.path)?;
    let result = process::capture(
        "git",
        &[
            "show".into(),
            format!("{}:{}", citation.commit, citation.path),
        ],
        &repository.checkout,
        Duration::from_secs(10),
    )?;
    ensure!(
        result.status.success(),
        "cited file is absent at pinned commit"
    );
    ensure!(
        storage::digest(&result.stdout) == citation.sha256,
        "cited file SHA-256 mismatch"
    );
    let text = std::str::from_utf8(&result.stdout).context("citation must be text")?;
    ensure!(
        citation.end_line <= text.lines().count(),
        "citation lines exceed file"
    );
    ensure!(
        !citation.quote.trim().is_empty() && citation.quote.len() <= 512,
        "citation needs a bounded exact quote"
    );
    let selected = text
        .lines()
        .skip(citation.start_line - 1)
        .take(citation.end_line - citation.start_line + 1)
        .collect::<Vec<_>>()
        .join("\n");
    ensure!(
        selected.contains(&citation.quote),
        "citation quote does not occur within pinned lines"
    );
    Ok(())
}

pub(super) fn regular_source(checkout: &Path, commit: &str, path: &str) -> Result<()> {
    safe_source_path(path)?;
    let result = process::capture(
        "git",
        &[
            "ls-tree".into(),
            "-z".into(),
            commit.into(),
            "--".into(),
            path.into(),
        ],
        checkout,
        Duration::from_secs(10),
    )?;
    ensure!(result.status.success(), "cannot inspect pinned source");
    let entries: Vec<_> = result
        .stdout
        .split(|b| *b == 0)
        .filter(|e| !e.is_empty())
        .collect();
    ensure!(
        entries.len() == 1,
        "source must be one existing regular file"
    );
    let entry = std::str::from_utf8(entries[0])?;
    let (metadata, found_path) = entry.split_once('\t').context("invalid Git tree entry")?;
    ensure!(
        found_path == path
            && (metadata.starts_with("100644 blob ") || metadata.starts_with("100755 blob ")),
        "source is not a regular Git blob"
    );
    Ok(())
}

pub(super) fn safe_source_path(path: &str) -> Result<()> {
    let parsed = Path::new(path);
    ensure!(
        !path.is_empty()
            && !parsed.is_absolute()
            && parsed
                .components()
                .all(|c| matches!(c, Component::Normal(_))),
        "source path escapes repository"
    );
    Ok(())
}

#[cfg(test)]
#[path = "citations_tests.rs"]
mod tests;
