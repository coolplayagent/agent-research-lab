//! Retained selection validation and read-only runtime prompt resolution.
use crate::files::read_strict;
use anyhow::{Context, Result, ensure};
use evolution::{PromptRegistry, PromptVersion};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectedStrategy {
    pub stable_version: String,
    pub task_kind: String,
    pub previous_stable_versions: Vec<String>,
    pub evidence_digest: String,
    pub policy_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StrategySelections {
    pub schema_version: u32,
    pub roles: BTreeMap<String, SelectedStrategy>,
}

impl Default for StrategySelections {
    fn default() -> Self {
        Self {
            schema_version: 1,
            roles: BTreeMap::new(),
        }
    }
}

impl StrategySelections {
    pub(super) fn validate(&self, registry: &PromptRegistry) -> Result<()> {
        ensure!(
            self.schema_version == 1,
            "unsupported strategy selection version"
        );
        registry.validate()?;
        for (role, selection) in &self.roles {
            ensure!(
                !selection.evidence_digest.is_empty() && !selection.policy_version.is_empty(),
                "selected strategy is missing evidence binding"
            );
            ensure!(
                selection.previous_stable_versions.len() <= 5,
                "strategy history exceeds five versions"
            );
            let mut seen = BTreeSet::new();
            for version in std::iter::once(&selection.stable_version)
                .chain(&selection.previous_stable_versions)
            {
                ensure!(
                    seen.insert(version),
                    "duplicate selected or historical strategy"
                );
                let prompt = registry
                    .get(version)
                    .context("selected strategy is not registered")?;
                ensure!(
                    prompt.definition().role == *role
                        && prompt.definition().task_kind == selection.task_kind,
                    "selected strategy role or task kind mismatch"
                );
            }
        }
        Ok(())
    }
}

/// New tasks freeze the selected version at enqueue time. Missing selection is
/// an initial-template fallback; corrupt state is an error, never a fallback.
pub fn resolve_selection(state_dir: &Path, role: &str) -> Result<Option<String>> {
    let path = state_dir.join("evolution/selections.json");
    if !path.exists() {
        return Ok(None);
    }
    let selections: StrategySelections = read_strict(&path)?;
    let registry: PromptRegistry = read_strict(&state_dir.join("evolution/prompts.json"))?;
    selections.validate(&registry)?;
    Ok(selections
        .roles
        .get(role)
        .map(|selection| selection.stable_version.clone()))
}

/// Resolve an explicitly selected immutable strategy for a runtime task. This
/// does not choose or activate a candidate on the caller's behalf.
pub fn resolve_registered_prompt(
    registry_file: &Path,
    version: &str,
    role: &str,
) -> Result<PromptVersion> {
    let registry: PromptRegistry = read_strict(registry_file)?;
    registry.validate()?;
    let prompt = registry
        .get(version)
        .context("selected prompt version is not registered")?;
    ensure!(
        prompt.definition().role == role,
        "selected prompt role does not match task role"
    );
    Ok(prompt.clone())
}

#[cfg(test)]
#[path = "selection_tests.rs"]
mod tests;
