//! Host lineage diagnostics and evidence-backed diagnostic receipts.
use crate::{
    files::{read_strict, write_optional},
    paths::relative_to,
};
use anyhow::{Result, ensure};
use evolution::PromptRegistry;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LineageRequest {
    pub registry_file: PathBuf,
    pub study: evolution::lineage::Study,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LineageReceipt {
    pub schema_version: u32,
    pub registry: PromptRegistry,
    pub study: evolution::lineage::Study,
    pub report: evolution::lineage::Report,
}

impl LineageReceipt {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.schema_version == 1, "unsupported lineage receipt");
        let recomputed = evolution::lineage::diagnose(&self.registry, &self.study)?;
        ensure!(
            serde_json::to_value(recomputed)? == serde_json::to_value(&self.report)?,
            "lineage receipt does not match its evidence"
        );
        Ok(())
    }
}

pub(super) fn run(input: &Path, base: &Path, output: Option<PathBuf>) -> Result<Value> {
    let request: LineageRequest = read_strict(input)?;
    let registry_path = relative_to(base, &request.registry_file).canonicalize()?;
    let registry: PromptRegistry = read_strict(&registry_path)?;
    let report = evolution::lineage::diagnose(&registry, &request.study)?;
    if let Some(destination) = &output {
        ensure!(
            destination.as_path() != input && destination != &registry_path,
            "lineage report cannot replace its inputs"
        );
    }
    let receipt = LineageReceipt {
        schema_version: 1,
        registry,
        study: request.study,
        report,
    };
    receipt.validate()?;
    write_optional(output.as_deref(), &receipt)?;
    Ok(serde_json::to_value(&receipt.report)?)
}

#[cfg(test)]
#[path = "lineage_tests.rs"]
mod tests;
