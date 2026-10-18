//! Host-owned evidence checks before admission, dispatch and explicit retry.
use crate::protocol::Operation;
use anyhow::{Context, Result, ensure};
use std::{fs, path::Path};

pub(super) fn validate_provenance(root: &Path, operation: &Operation) -> Result<()> {
    let evidence = match operation {
        Operation::Install(request) => Some(&request.evidence),
        Operation::Merge(request) => Some(&request.evidence),
        _ => None,
    };
    if let Some(evidence) = evidence {
        let trusted = fs::canonicalize(root.join("evidence"))?;
        for artifact in [
            &evidence.qualitygate_report,
            &evidence.snapshot_receipt,
            &evidence.evaluation_receipt,
            evidence
                .skills_manifest
                .as_ref()
                .context("latest skills evidence is required")?,
        ] {
            ensure!(
                fs::canonicalize(&artifact.path)?.starts_with(&trusted),
                "delivery evidence must be host-owned under outbox/evidence"
            );
        }
        evidence.validate()?;
    }
    if let Operation::KnowledgePrepare(request) = operation {
        knowledge::validate_report(&request.report, &request.knowledge_root)?;
    }
    Ok(())
}
