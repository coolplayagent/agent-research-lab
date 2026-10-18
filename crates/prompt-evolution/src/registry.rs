//! Immutable prompt archive, ancestry validation and diverse portfolio membership.
use crate::identity::{digest, nonempty};
use anyhow::{Context, Result, ensure};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// The immutable payload of a prompt. Changing any field creates a new version.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PromptDefinition {
    pub parent_versions: Vec<String>,
    pub role: String,
    pub task_kind: String,
    pub content: String,
    pub change_reason: String,
    pub failure_conditions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PromptVersion {
    version: String,
    definition: PromptDefinition,
    created_at: DateTime<Utc>,
}

impl PromptVersion {
    pub fn new(mut definition: PromptDefinition) -> Result<Self> {
        nonempty(&definition.role, "role")?;
        nonempty(&definition.task_kind, "task kind")?;
        nonempty(&definition.content, "prompt content")?;
        nonempty(&definition.change_reason, "change reason")?;
        definition.parent_versions.sort();
        ensure!(
            definition.parent_versions.windows(2).all(|p| p[0] != p[1]),
            "duplicate prompt parent"
        );
        ensure!(
            !definition.failure_conditions.is_empty(),
            "at least one failure condition is required"
        );
        for condition in &definition.failure_conditions {
            nonempty(condition, "failure condition")?;
        }
        Ok(Self {
            version: digest(&definition)?,
            definition,
            created_at: Utc::now(),
        })
    }

    pub fn version(&self) -> &str {
        &self.version
    }

    pub fn definition(&self) -> &PromptDefinition {
        &self.definition
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    pub fn validate(&self) -> Result<()> {
        let rebuilt = Self::new(self.definition.clone())?;
        ensure!(
            self.version == rebuilt.version && self.definition == rebuilt.definition,
            "prompt version does not match its immutable payload"
        );
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PromptRegistry {
    versions: BTreeMap<String, PromptVersion>,
}

impl PromptRegistry {
    pub(super) fn versions(&self) -> &BTreeMap<String, PromptVersion> {
        &self.versions
    }

    /// An identical version is idempotent; existing content is never overwritten.
    pub fn register(&mut self, prompt: PromptVersion) -> Result<String> {
        self.validate()?;
        prompt.validate()?;
        for parent in &prompt.definition.parent_versions {
            let ancestor = self.versions.get(parent).context("unknown prompt parent")?;
            ensure!(
                ancestor.definition.task_kind == prompt.definition.task_kind,
                "prompt parent belongs to a different task kind"
            );
        }
        let version = prompt.version.clone();
        if let Some(existing) = self.versions.get(&version) {
            ensure!(
                existing.definition == prompt.definition,
                "prompt hash collision"
            );
        } else {
            self.versions.insert(version.clone(), prompt);
        }
        Ok(version)
    }

    pub fn get(&self, version: &str) -> Option<&PromptVersion> {
        self.versions.get(version)
    }

    pub fn validate(&self) -> Result<()> {
        for (key, prompt) in &self.versions {
            prompt.validate()?;
            ensure!(key == &prompt.version, "prompt registry key mismatch");
            for parent in &prompt.definition.parent_versions {
                let ancestor = self
                    .versions
                    .get(parent)
                    .context("missing prompt ancestor")?;
                ensure!(
                    ancestor.definition.task_kind == prompt.definition.task_kind,
                    "prompt ancestry changed task kind"
                );
                ensure!(parent != key, "prompt cannot be its own parent");
            }
        }
        // Content addressing makes a valid cycle infeasible, but explicitly
        // check the graph as well when loading untrusted serialized registries.
        let mut completed = BTreeSet::new();
        let mut visiting = BTreeSet::new();
        for version in self.versions.keys() {
            self.visit(version, &mut visiting, &mut completed)?;
        }
        Ok(())
    }

    fn visit<'a>(
        &'a self,
        version: &'a str,
        visiting: &mut BTreeSet<&'a str>,
        completed: &mut BTreeSet<&'a str>,
    ) -> Result<()> {
        if completed.contains(version) {
            return Ok(());
        }
        ensure!(visiting.insert(version), "cyclic prompt ancestry");
        for parent in &self.versions[version].definition.parent_versions {
            self.visit(parent, visiting, completed)?;
        }
        visiting.remove(version);
        completed.insert(version);
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyPortfolio {
    pub task_kind: String,
    pub stable: String,
    pub specialized: Vec<String>,
    pub exploratory: Vec<String>,
}

impl StrategyPortfolio {
    pub fn validate(&self, registry: &PromptRegistry) -> Result<()> {
        registry.validate()?;
        nonempty(&self.task_kind, "task kind")?;
        ensure!(
            self.specialized.len() <= 2,
            "at most two specialized strategies"
        );
        ensure!(
            self.exploratory.len() <= 2,
            "at most two exploratory strategies"
        );
        let mut seen = BTreeSet::new();
        for version in std::iter::once(&self.stable)
            .chain(self.specialized.iter())
            .chain(self.exploratory.iter())
        {
            ensure!(
                seen.insert(version),
                "a strategy occupies multiple portfolio slots"
            );
            let prompt = registry.get(version).context("unknown portfolio prompt")?;
            ensure!(
                prompt.definition.task_kind == self.task_kind,
                "portfolio prompt belongs to a different task kind"
            );
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "registry_tests.rs"]
mod tests;
