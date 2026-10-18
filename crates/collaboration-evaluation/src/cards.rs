//! Decode untrusted worker cards and enforce supplied dependency references.
use crate::{
    identity::digest,
    protocol::{Change, ClaimCard, CollectedClaim, PlannedTask},
};
use anyhow::{Context, Result, ensure};
use config::safe_id;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeSet;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkerReport {
    summary: String,
    findings: Vec<String>,
    sources: Vec<String>,
    limitations: Vec<String>,
    next_tasks: Vec<Value>,
}

pub(super) fn report_cards(
    entry: &PlannedTask,
    receipt: &Value,
    max: usize,
    known: &BTreeSet<String>,
) -> Result<Vec<CollectedClaim>> {
    let report: WorkerReport = serde_json::from_value(receipt["agent_report"].clone())?;
    ensure!(
        !report.summary.trim().is_empty() && report.next_tasks.is_empty(),
        "invalid summary or unauthorized follow-ups"
    );
    ensure!(
        report.findings.len() <= max
            && report.sources.len() <= 64
            && report.limitations.len() <= 32,
        "worker report exceeds frozen bounds"
    );
    let mut ids = BTreeSet::new();
    let receipt_digest = digest(receipt)?;
    let mut claims = Vec::new();
    for finding in report.findings {
        ensure!(finding.len() <= 32768, "claim card exceeds byte bound");
        let card: ClaimCard =
            serde_json::from_str(&finding).context("findings must contain JSON claim cards")?;
        safe_id(&card.id)?;
        ensure!(ids.insert(card.id.clone()), "duplicate claim ID");
        ensure!(
            !card.text.trim().is_empty() && card.text.len() <= 2048 && card.rationale.len() <= 2048,
            "invalid claim text"
        );
        ensure!(
            card.evidence.len() <= 8
                && card.counterexample_to.len() <= 8
                && card.revises.len() <= 8,
            "claim exceeds evidence/reference bound"
        );
        if entry.round == 1 {
            ensure!(
                card.change == Change::New
                    && card.revises.is_empty()
                    && card.counterexample_to.is_empty(),
                "initial claim cannot refer to other workers"
            );
        }
        ensure!(
            (card.change == Change::New) == card.revises.is_empty(),
            "revision relation inconsistent with change type"
        );
        for reference in card.counterexample_to.iter().chain(&card.revises) {
            let (task, local) = reference
                .split_once('#')
                .context("reference must be task-id#claim-id")?;
            safe_id(local)?;
            ensure!(
                entry.task.dependencies.iter().any(|id| id == task) && known.contains(reference),
                "reference is unavailable, cross-arm or uncommitted"
            );
        }
        for reference in &card.revises {
            ensure!(
                reference.split_once('#').map(|v| v.0)
                    == entry.task.dependencies.first().map(String::as_str),
                "revision must refer to the worker's own initial claim"
            );
        }
        let id = format!("{}#{}", entry.task.id, card.id);
        claims.push(CollectedClaim {
            id,
            digest: digest(&json!({"receipt":receipt_digest,"card":card}))?,
            arm: entry.arm,
            round: entry.round,
            card,
            citation_errors: vec![],
            adjudication: None,
        });
    }
    Ok(claims)
}
