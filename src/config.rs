use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema_version: u32,
    pub workspace: PathBuf,
    pub state_dir: PathBuf,
    pub superpod: PathBuf,
    pub codex: String,
    pub workflow: PathBuf,
    pub daily_seconds: u64,
    pub max_agents: usize,
    pub task_timeout_seconds: u64,
    pub models: BTreeMap<String, String>,
    pub tools: BTreeMap<String, Tool>,
    /// Production defaults to live upstream checks. Explicitly disabled only in offline fixtures.
    #[serde(default = "latest_default")]
    pub require_latest: bool,
    #[serde(default)]
    pub skills_manifest: Option<PathBuf>,
}
fn latest_default() -> bool {
    true
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tool {
    pub binary: PathBuf,
    pub repository: PathBuf,
    pub probe: Vec<String>,
}
impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let c: Self = toml::from_str(&std::fs::read_to_string(path)?)?;
        c.validate()?;
        Ok(c)
    }
    pub fn validate(&self) -> Result<()> {
        if self.require_latest
            && self
                .skills_manifest
                .as_ref()
                .is_none_or(|p| !p.is_absolute())
        {
            bail!("latest-baseline mode requires an absolute skills_manifest path");
        }
        if self.schema_version != 1
            || self.max_agents == 0
            || self.max_agents > 3
            || self.daily_seconds == 0
            || self.daily_seconds > 43200
            || self.task_timeout_seconds == 0
            || self.task_timeout_seconds > self.daily_seconds
        {
            bail!(
                "invalid version, concurrency, or budget; maximum 3 agents / 43200 daily seconds"
            );
        }
        for p in [
            &self.workspace,
            &self.state_dir,
            &self.superpod,
            &self.workflow,
        ] {
            if !p.is_absolute() {
                bail!("machine paths must be absolute: {}", p.display());
            }
        }
        if self.state_dir.starts_with(&self.superpod) {
            bail!("private runtime state cannot live in SuperPOD");
        }
        for role in ["research", "implement", "review"] {
            if self.models.get(role).is_none_or(|m| m.trim().is_empty()) {
                bail!("missing model for {role}");
            }
        }
        for name in [
            "workflow-cli",
            "relay-knowledge",
            "into-markdown",
            "qualitygate-cli",
            "computer-use-cli",
            "relay-memory",
            "repo-sandbox",
        ] {
            if !self.tools.contains_key(name) {
                bail!("missing tool binding: {name}");
            }
        }
        Ok(())
    }
}

pub fn safe_id(s: &str) -> Result<()> {
    if s.is_empty()
        || s.len() > 100
        || !s
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    {
        bail!("identifier must be 1..100 ASCII letters, digits, underscores or hyphens");
    }
    Ok(())
}
