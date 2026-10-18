//! Build and verify the exact merged snapshot before activation can proceed.
use crate::{
    host::{command, text},
    journal::phase,
    model::Prepared,
};
use anyhow::{Context, Result, ensure};
use config::Config;
use contracts::ActivationPhase;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn clean(source: &Path, expected: &str) -> Result<()> {
    ensure!(
        text(&["git".into(), "rev-parse".into(), "HEAD".into()], source)? == expected
            && text(
                &["git".into(), "status".into(), "--porcelain".into()],
                source
            )?
            .is_empty(),
        "activation build modified the verified source snapshot"
    );
    Ok(())
}

pub(super) fn build(
    operation: &Path,
    prepared: &Prepared,
    config: &Config,
) -> Result<(PathBuf, Value)> {
    phase(operation, ActivationPhase::Building, json!({}))?;
    let request = &prepared.request;
    let repository = &prepared.repository;
    let remote = freshness::RemoteClient::default().remote_snapshot(&repository.checkout)?;
    ensure!(
        remote.repository == request.repository && remote.commit == request.merged_commit,
        "merged activation source is no longer the upstream default"
    );
    command(
        &[
            "git".into(),
            "fetch".into(),
            "origin".into(),
            request.merged_commit.clone(),
            request.candidate_commit.clone(),
        ],
        &repository.checkout,
        120,
    )?;
    let source = operation.join("source");
    ensure!(
        !source.exists(),
        "activation source already exists; reconcile the interrupted operation"
    );
    command(
        &[
            "git".into(),
            "worktree".into(),
            "add".into(),
            "--detach".into(),
            source.display().to_string(),
            request.merged_commit.clone(),
        ],
        &repository.checkout,
        120,
    )?;
    let tree = text(
        &[
            "git".into(),
            "rev-parse".into(),
            format!("{}^{{tree}}", request.merged_commit),
        ],
        &source,
    )?;
    ensure!(
        tree == text(
            &[
                "git".into(),
                "rev-parse".into(),
                format!("{}^{{tree}}", request.candidate_commit)
            ],
            &source
        )?,
        "merged tree differs from the independently verified candidate"
    );
    for (index, args) in repository.build_commands.iter().enumerate() {
        fs::write(
            operation.join(format!("build-{index}.log")),
            command(args, &source, 1800)?,
        )?;
    }
    clean(&source, &request.merged_commit)?;
    let qg = config
        .tools
        .get("qualitygate-cli")
        .context("Qualitygate unavailable")?
        .binary
        .display()
        .to_string();
    let report: Value = serde_json::from_slice(&command(
        &repository.qualitygate_command(qg, &source, &request.merged_commit),
        &source,
        1800,
    )?)?;
    validate_gate(&report, &request.merged_commit)?;
    clean(&source, &request.merged_commit)?;
    storage::write(&operation.join("merged-qualitygate.json"), &report)?;
    freshness::RemoteClient::default().verify_remote_binding(&remote)?;
    Ok((source, report))
}

fn validate_gate(report: &Value, commit: &str) -> Result<()> {
    ensure!(
        report["profile"] == "full"
            && matches!(report["scope"].as_str(), Some("delivery" | "task"))
            && report["gate"]["complete"] == true
            && report["gate"]["decision"] == contracts::VerificationVerdict::Pass.as_str()
            && report["selection"]["empty_delivery"] == false
            && report["snapshot"]["mode"] == "diff"
            && report["snapshot"]["head"] == commit
            && report["plan"]["pending_delivery_checks"]
                .as_array()
                .is_some_and(Vec::is_empty),
        "activation requires an exact merged-commit complete nonempty full gate"
    );
    Ok(())
}

#[cfg(test)]
#[path = "build_tests.rs"]
mod tests;
