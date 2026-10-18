//! Prepare a reviewable report and routing contribution in an isolated worktree.
use crate::{
    host::{git, relay},
    protocol::{PrepareReportRequest, PreparedReport},
    report::render_report,
    snapshot::publication_snapshot,
    validation::validate_report,
};
use anyhow::{Context, Result, ensure};
use delivery::file_sha256;
use std::{collections::BTreeMap, fs, path::Path};

fn add_index_line(path: &Path, heading: &str, line: &str) -> Result<()> {
    let mut text = if path.exists() {
        fs::read_to_string(path)?
    } else {
        format!("{heading}\n")
    };
    if !text.lines().any(|existing| existing == line) {
        text.push_str(&format!("\n{line}\n"));
        fs::write(path, text)?;
    }
    Ok(())
}

/// Prepare a reviewable contribution on an isolated branch; never mutate external knowledge repository's current checkout.
pub fn prepare_report(request: &PrepareReportRequest) -> Result<PreparedReport> {
    ensure!(
        request.knowledge_root.is_absolute() && request.worktree.is_absolute(),
        "external knowledge repository and worktree paths must be absolute"
    );
    request.layout.validate()?;
    ensure!(
        git(&request.knowledge_root, &["remote", "get-url", "origin"])?
            == request.layout.expected_origin,
        "publication origin mismatch"
    );
    validate_report(&request.report, &request.knowledge_root)?;
    let baseline = &request.report.bindings.knowledge_revision;
    let resolved = git(
        &request.knowledge_root,
        &["rev-parse", &format!("{baseline}^{{commit}}")],
    )?;
    ensure!(
        &resolved == baseline,
        "external knowledge repository baseline does not resolve exactly"
    );
    let branch = format!("outcomes/{}", request.report.bindings.experiment_id);
    if request.worktree.exists() {
        let source_common = fs::canonicalize(request.knowledge_root.join(git(
            &request.knowledge_root,
            &["rev-parse", "--git-common-dir"],
        )?))?;
        let worktree_common = fs::canonicalize(
            request
                .worktree
                .join(git(&request.worktree, &["rev-parse", "--git-common-dir"])?),
        )?;
        ensure!(
            source_common == worktree_common,
            "existing worktree belongs to another repository"
        );
        ensure!(
            git(&request.worktree, &["branch", "--show-current"])? == branch,
            "worktree belongs to a different branch"
        );
        ensure!(
            git(&request.worktree, &["rev-parse", "HEAD"])? == *baseline,
            "worktree advanced; reconcile previous publication first"
        );
    } else {
        let worktree = request
            .worktree
            .to_str()
            .context("non-UTF-8 worktree path")?;
        git(
            &request.knowledge_root,
            &["worktree", "add", "-b", &branch, worktree, baseline],
        )?;
    }
    validate_report(&request.report, &request.worktree)?;
    relay(
        &request.relay_knowledge_binary,
        &request.worktree,
        &["map", "validate", "--format", "json"],
    )?;
    let folder = request.worktree.join(&request.layout.report_directory);
    fs::create_dir_all(&folder)?;
    let relative = request
        .layout
        .report_directory
        .join(format!("{}.md", request.report.bindings.experiment_id));
    let report_path = request.worktree.join(&relative);
    let rendered = render_report(&request.report);
    if report_path.exists() {
        ensure!(
            fs::read_to_string(&report_path)? == rendered,
            "existing report differs; use a new experiment ID or review an explicit revision"
        );
    } else {
        fs::write(&report_path, rendered)?;
    }
    add_index_line(
        &folder.join("index.md"),
        "# Published outcomes",
        &format!(
            "- [{}]({}.md)",
            request.report.title.replace(['\n', '\r'], " "),
            request.report.bindings.experiment_id
        ),
    )?;
    // Routing artifacts are generated exclusively by the published CLI.
    let source_id = format!("outcome-{}", request.report.bindings.experiment_id);
    let shown = relay(
        &request.relay_knowledge_binary,
        &request.worktree,
        &["map", "show", "--type", "knowledge", "--format", "json"],
    )?;
    let sources = shown["map"]["sources"]
        .as_array()
        .context("map source inventory missing")?;
    if let Some(existing) = sources.iter().find(|source| source["id"] == source_id) {
        ensure!(
            existing["uri"].as_str() == relative.to_str()
                && existing["topic"] == request.layout.topic,
            "existing knowledge source ID has another route"
        );
    } else {
        relay(
            &request.relay_knowledge_binary,
            &request.worktree,
            &[
                "map",
                "source",
                "add",
                "--type",
                "knowledge",
                "--id",
                &source_id,
                "--topic",
                &request.layout.topic,
                "--kind",
                "file",
                "--uri",
                relative.to_str().context("invalid report path")?,
                "--description",
                &request.report.title,
                "--format",
                "json",
            ],
        )?;
    }
    relay(
        &request.relay_knowledge_binary,
        &request.worktree,
        &["map", "validate", "--format", "json"],
    )?;
    let mut prepared = PreparedReport {
        layout: request.layout.clone(),
        worktree: fs::canonicalize(&request.worktree)?,
        branch,
        baseline: baseline.clone(),
        report_path: relative,
        report_sha256: file_sha256(&report_path)?,
        experiment_id: request.report.bindings.experiment_id.clone(),
        relay_knowledge_binary: request.relay_knowledge_binary.clone(),
        publication_files: BTreeMap::new(),
    };
    prepared.publication_files = publication_snapshot(&prepared, None)?;
    Ok(prepared)
}

#[cfg(test)]
#[path = "preparation_tests.rs"]
mod tests;
