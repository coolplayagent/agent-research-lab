//! Reviewed-public-content, evidence-binding and source-archive validation.
use crate::report::OutcomeReport;
use anyhow::{Context, Result, ensure};
use contracts::{FindingKind, SourceAccessStatus};
use delivery::{file_sha256, relative_path, validate_commit, validate_digest, validate_identifier};
use std::{collections::BTreeSet, fs, path::Path};

fn public_text(text: &str) -> Result<()> {
    let lower = text.to_ascii_lowercase();
    ensure!(
        !text.contains('\0')
            && !lower.contains("-----begin private key-----")
            && !lower.contains("-----begin openssh private key-----")
            && !lower.contains("ghp_")
            && !lower.contains("github_pat_")
            && !lower.contains("sk-proj-")
            && !lower.contains("authorization: bearer")
            && !lower.contains(".codex/auth.json"),
        "publication contains credential-like or private configuration content"
    );
    // This is a tripwire, not a claim that arbitrary prose can be automatically declassified.
    ensure!(
        !lower.contains("holdout_answer")
            && !lower.contains("private_holdout")
            && !lower.contains("raw_session_log"),
        "private evaluation or raw session content cannot be published"
    );
    Ok(())
}

pub fn validate_report(report: &OutcomeReport, knowledge_root: &Path) -> Result<()> {
    ensure!(
        report.publication_reviewed,
        "publication content review is required"
    );
    validate_identifier(&report.bindings.experiment_id)?;
    ensure!(
        report.bindings.experiment_id != "index",
        "experiment ID is reserved for navigation"
    );
    validate_commit(&report.bindings.source_commit)?;
    validate_commit(&report.bindings.knowledge_revision)?;
    validate_digest(&report.bindings.prompt_sha256)?;
    validate_digest(&report.bindings.policy_sha256)?;
    ensure!(
        report
            .bindings
            .source_repository
            .starts_with("https://github.com/")
            && !report
                .bindings
                .source_repository
                .contains(['?', '#', '@', '\n']),
        "source repository must be a public GitHub URL"
    );
    ensure!(
        !report.title.trim().is_empty()
            && !report.summary.trim().is_empty()
            && !report.findings.is_empty(),
        "report needs title, summary and findings"
    );
    public_text(&serde_json::to_string(report)?)?;
    let mut ids = BTreeSet::new();
    for source in &report.sources {
        validate_identifier(&source.id)?;
        if let Some(digest) = &source.sha256 {
            validate_digest(digest)?;
        }
        ensure!(ids.insert(source.id.clone()), "duplicate source ID");
        ensure!(!source.title.trim().is_empty(), "source title is required");
        chrono::DateTime::parse_from_rfc3339(&source.retrieved_at)
            .context("source retrieval time must be RFC3339")?;
        if source.resource.starts_with("https://") {
            ensure!(
                source.access_status != SourceAccessStatus::DownloadedVerified,
                "downloaded-verified sources must identify their local archive"
            );
            let authority = source
                .resource
                .trim_start_matches("https://")
                .split('/')
                .next()
                .unwrap_or("");
            ensure!(
                !authority.is_empty()
                    && !source.resource.contains(['@', '?', '#', '\n', '\r', ' '])
                    && authority != "localhost"
                    && !authority.starts_with("127."),
                "source URL must be a normalized public URL without credentials or query tokens"
            );
        } else {
            let path = Path::new(&source.resource);
            relative_path(path)?;
            ensure!(
                path.starts_with("sources"),
                "local sources must use external knowledge repository's sources archive"
            );
            let resolved =
                fs::canonicalize(knowledge_root.join(path)).context("source archive is missing")?;
            ensure!(
                resolved.starts_with(fs::canonicalize(knowledge_root.join("sources"))?),
                "source escapes archive"
            );
            ensure!(
                Some(file_sha256(&resolved)?) == source.sha256,
                "source archive checksum mismatch"
            );
        }
    }
    let mut used = BTreeSet::new();
    for finding in &report.findings {
        ensure!(
            !finding.statement.trim().is_empty() && !finding.limitations.trim().is_empty(),
            "findings require a statement and limitations"
        );
        for id in &finding.source_ids {
            ensure!(ids.contains(id), "finding cites unknown source: {id}");
            used.insert(id.clone());
        }
        match finding.kind {
            FindingKind::Measured => {
                ensure!(
                    !finding.method.trim().is_empty(),
                    "measured claims require a method"
                );
                validate_digest(
                    finding
                        .public_evidence_sha256
                        .as_deref()
                        .context("measured claims require public evidence digest")?,
                )?;
            }
            FindingKind::ExternalClaim | FindingKind::Inference => ensure!(
                !finding.source_ids.is_empty(),
                "external claims and inferences require citations"
            ),
            FindingKind::Unknown => {}
        }
    }
    ensure!(
        ids == used,
        "every listed source must be cited by a finding"
    );
    Ok(())
}

#[cfg(test)]
#[path = "validation_tests.rs"]
mod tests;
