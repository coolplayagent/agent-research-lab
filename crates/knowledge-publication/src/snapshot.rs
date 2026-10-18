//! Bind the full contribution to reviewed paths, file bytes and baseline ancestry.
use crate::{
    host::git,
    protocol::{PreparedReport, PublishReportRequest},
};
use anyhow::{Context, Result, ensure};
use delivery::{file_sha256, relative_path, validate_commit, validate_digest, validate_identifier};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

pub(super) fn allowed_publication_path(path: &Path, report: &PreparedReport) -> bool {
    path == report.report_path
        || path == report.layout.report_directory.join("index.md")
        || report
            .layout
            .navigation_files
            .iter()
            .any(|allowed| path == allowed)
        || report
            .layout
            .generated_roots
            .iter()
            .any(|root| path.starts_with(root))
}

fn nul_paths(output: std::process::Output) -> Result<Vec<PathBuf>> {
    ensure!(output.status.success(), "cannot inspect publication diff");
    Ok(String::from_utf8(output.stdout)?
        .split('\0')
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .collect())
}

pub(super) fn publication_snapshot(
    prepared: &PreparedReport,
    commit: Option<&str>,
) -> Result<BTreeMap<PathBuf, Option<String>>> {
    let mut args = vec![
        "diff".into(),
        "--name-only".into(),
        "-z".into(),
        prepared.baseline.clone(),
    ];
    if let Some(commit) = commit {
        args.push(commit.into());
    }
    args.push("--".into());
    let mut paths: BTreeSet<PathBuf> = nul_paths(process::capture(
        "git",
        &args,
        &prepared.worktree,
        std::time::Duration::from_secs(30),
    )?)?
    .into_iter()
    .collect();
    if commit.is_none() {
        for args in [
            vec!["ls-files", "--others", "--exclude-standard", "-z"],
            vec!["diff", "--cached", "--name-only", "-z", "--"],
        ] {
            paths.extend(nul_paths(process::capture(
                "git",
                &args.into_iter().map(String::from).collect::<Vec<_>>(),
                &prepared.worktree,
                std::time::Duration::from_secs(30),
            )?)?);
        }
    }
    let mut result = BTreeMap::new();
    for path in paths {
        relative_path(&path)?;
        ensure!(
            allowed_publication_path(&path, prepared),
            "unexpected publication file: {}",
            path.display()
        );
        let digest = if let Some(commit) = commit {
            let entry = git(
                &prepared.worktree,
                &[
                    "ls-tree",
                    commit,
                    "--",
                    path.to_str().context("non-UTF-8 publication path")?,
                ],
            )?;
            if entry.is_empty() {
                None
            } else {
                ensure!(
                    entry.starts_with("100644 blob ") || entry.starts_with("100755 blob "),
                    "publication tree entry must be a regular file"
                );
                let output = process::capture(
                    "git",
                    &["show".into(), format!("{commit}:{}", path.display())],
                    &prepared.worktree,
                    std::time::Duration::from_secs(30),
                )?;
                ensure!(
                    output.status.success(),
                    "cannot read committed publication file"
                );
                Some(format!("{:x}", Sha256::digest(&output.stdout)))
            }
        } else {
            let file = prepared.worktree.join(&path);
            match fs::symlink_metadata(&file) {
                Ok(metadata) => {
                    ensure!(
                        metadata.file_type().is_file() && fs::canonicalize(&file)? == file,
                        "publication path must be a regular file without symlink aliases"
                    );
                    Some(file_sha256(&file)?)
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => return Err(error.into()),
            }
        };
        result.insert(path, digest);
    }
    Ok(result)
}

pub(super) fn validate_prepared_publication(request: &PublishReportRequest) -> Result<()> {
    let prepared = &request.prepared;
    prepared.layout.validate()?;
    validate_identifier(&prepared.experiment_id)?;
    validate_commit(&prepared.baseline)?;
    validate_digest(&prepared.report_sha256)?;
    ensure!(prepared.experiment_id != "index", "reserved experiment ID");
    let expected_branch = format!("outcomes/{}", prepared.experiment_id);
    ensure!(
        prepared.branch == expected_branch && prepared.branch != request.base_branch,
        "publication must use its dedicated outcome branch, distinct from the base"
    );
    git(
        &prepared.worktree,
        &["check-ref-format", "--branch", &request.base_branch],
    )?;
    ensure!(
        git(&prepared.worktree, &["branch", "--show-current"])? == expected_branch,
        "publication branch changed"
    );
    ensure!(
        prepared.report_path
            == prepared
                .layout
                .report_directory
                .join(format!("{}.md", prepared.experiment_id)),
        "report path does not match the experiment"
    );
    git(
        &prepared.worktree,
        &["merge-base", "--is-ancestor", &prepared.baseline, "HEAD"],
    )?;
    ensure!(
        prepared.publication_files.get(&prepared.report_path)
            == Some(&Some(prepared.report_sha256.clone())),
        "review manifest does not bind the report"
    );
    ensure!(
        publication_snapshot(prepared, None)? == prepared.publication_files,
        "publication snapshot changed after review"
    );
    Ok(())
}

#[cfg(test)]
#[path = "snapshot_tests.rs"]
mod tests;
