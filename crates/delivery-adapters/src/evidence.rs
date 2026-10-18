//! Validate host evidence and exact source, prompt, policy and knowledge bindings.
use crate::validation::{validate_commit, validate_digest};
use anyhow::{Context, Result, ensure};
use freshness::{RemoteClient, RemoteSnapshot, SkillsManifest};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fs, path::PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VerificationArtifact {
    pub path: PathBuf,
    pub sha256: String,
}

impl VerificationArtifact {
    pub fn read(&self) -> Result<Value> {
        validate_digest(&self.sha256)?;
        ensure!(
            fs::symlink_metadata(&self.path)?.file_type().is_file(),
            "verification evidence must be a regular file"
        );
        let bytes = fs::read(&self.path)?;
        ensure!(
            format!("{:x}", Sha256::digest(&bytes)) == self.sha256,
            "verification artifact changed"
        );
        serde_json::from_slice(&bytes).context("verification artifact must be JSON")
    }

    pub fn validate_skills(&self) -> Result<()> {
        let manifest: SkillsManifest = serde_json::from_value(self.read()?)?;
        ensure!(
            manifest.schema_version == 1 && !manifest.skills.is_empty(),
            "skills manifest must be a nonempty version-one host artifact"
        );
        Ok(())
    }

    pub fn verify_skills(&self, remote: &RemoteClient) -> Result<()> {
        self.validate_skills()?;
        remote.verify_skills(&self.path)?;
        // Do not permit a replacement between the bound hash check and the
        // live verification to silently switch which skills were evaluated.
        self.validate_skills()?;
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReleaseEvidence {
    pub source_commit: String,
    /// Optional only for reading historical receipts. Every new delivery gate
    /// requires a bound, currently fresh remote-default baseline.
    #[serde(default)]
    pub upstream: Option<RemoteSnapshot>,
    #[serde(default)]
    pub skills_manifest: Option<VerificationArtifact>,
    pub prompt_sha256: String,
    pub policy_sha256: String,
    pub superpod_commit: String,
    pub implementer_id: String,
    pub evaluator_id: String,
    pub evaluation_passed: bool,
    pub qualitygate_passed: bool,
    pub qualitygate_report: VerificationArtifact,
    /// Host-produced snapshot binding; candidate agents must not own this receipt.
    pub snapshot_receipt: VerificationArtifact,
    /// Host-collected result of a distinct evaluator invocation, with exact input bindings.
    pub evaluation_receipt: VerificationArtifact,
}

impl ReleaseEvidence {
    pub fn validate(&self) -> Result<()> {
        let upstream = self.upstream.as_ref().context(
            "old release evidence lacks an upstream baseline; re-evaluation is required",
        )?;
        upstream.validate()?;
        let skills = self.skills_manifest.as_ref().context(
            "old release evidence lacks a hashed skills manifest; re-evaluation is required",
        )?;
        skills.validate_skills()?;
        validate_commit(&self.source_commit)?;
        validate_commit(&self.superpod_commit)?;
        validate_digest(&self.prompt_sha256)?;
        validate_digest(&self.policy_sha256)?;
        ensure!(
            self.evaluation_passed && self.qualitygate_passed,
            "independent evaluation and Qualitygate must pass"
        );
        ensure!(
            !self.implementer_id.is_empty()
                && !self.evaluator_id.is_empty()
                && self.implementer_id != self.evaluator_id,
            "evaluation must identify a distinct evaluator"
        );
        let qualitygate = self.qualitygate_report.read()?;
        ensure!(
            qualitygate["profile"] == "full"
                && ["repository", "delivery", "task"]
                    .contains(&qualitygate["scope"].as_str().unwrap_or("")),
            "Qualitygate report must cover a full delivery, task or repository scope"
        );
        ensure!(
            qualitygate["gate"]["complete"] == true
                && qualitygate["gate"]["decision"] == contracts::VerificationVerdict::Pass.as_str(),
            "Qualitygate report is incomplete or failed"
        );
        ensure!(
            qualitygate["plan"]["pending_delivery_checks"]
                .as_array()
                .is_some_and(Vec::is_empty),
            "Qualitygate has pending or unknown checks"
        );
        let snapshot = self.snapshot_receipt.read()?;
        let evaluation = self.evaluation_receipt.read()?;
        for receipt in [&snapshot, &evaluation] {
            ensure!(
                receipt["skills_manifest_sha256"].as_str() == Some(&skills.sha256),
                "verification receipt has stale skills manifest binding"
            );
            let bound: RemoteSnapshot = serde_json::from_value(receipt["upstream"].clone())
                .context("verification receipt lacks a valid upstream baseline")?;
            bound.validate()?;
            ensure!(
                bound.same_binding(upstream),
                "verification receipt has stale upstream baseline"
            );
            for (key, expected) in [
                ("source_commit", &self.source_commit),
                ("prompt_sha256", &self.prompt_sha256),
                ("policy_sha256", &self.policy_sha256),
                ("superpod_commit", &self.superpod_commit),
            ] {
                ensure!(
                    receipt[key].as_str() == Some(expected),
                    "verification receipt has stale {key}"
                );
            }
        }
        ensure!(
            snapshot["qualitygate_report_sha256"].as_str() == Some(&self.qualitygate_report.sha256),
            "snapshot receipt refers to another Qualitygate report"
        );
        ensure!(
            snapshot["content_digest"].is_string()
                && snapshot["content_digest"] == qualitygate["snapshot"]["content_digest"]
                && snapshot["verification_digest"]
                    == qualitygate["snapshot"]["verification_digest"],
            "Qualitygate snapshot differs from host-collected source snapshot"
        );
        ensure!(
            evaluation["role"] == "independent_evaluator"
                && evaluation["status"] == contracts::VerificationVerdict::Pass.as_str()
                && evaluation["complete"] == true,
            "independent evaluation receipt is incomplete or failed"
        );
        ensure!(
            evaluation["evaluator_id"].as_str() == Some(&self.evaluator_id)
                && evaluation["implementer_id"].as_str() == Some(&self.implementer_id),
            "evaluator invocation identity mismatch"
        );
        Ok(())
    }
}

#[cfg(test)]
#[path = "evidence_tests.rs"]
mod tests;
