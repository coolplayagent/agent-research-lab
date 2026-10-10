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
            || self.max_agents > 8
            || self.daily_seconds == 0
            || self.daily_seconds > 43200
            || self.task_timeout_seconds == 0
            || self.task_timeout_seconds > self.daily_seconds
        {
            bail!(
                "invalid version, concurrency, or budget; maximum 8 agents / 43200 daily seconds"
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

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_config() -> Config {
        Config {
            schema_version: 1,
            workspace: "/workspace".into(),
            state_dir: "/workspace/lab/.lab".into(),
            superpod: "/workspace/superpod".into(),
            codex: "codex".into(),
            workflow: "/tools/workflow".into(),
            daily_seconds: 43200,
            max_agents: 8,
            task_timeout_seconds: 1800,
            models: ["research", "implement", "review"]
                .into_iter()
                .map(|role| (role.into(), "fixture-model".into()))
                .collect(),
            tools: [
                "workflow-cli",
                "relay-knowledge",
                "into-markdown",
                "qualitygate-cli",
                "computer-use-cli",
                "relay-memory",
                "repo-sandbox",
            ]
            .into_iter()
            .map(|name| {
                (
                    name.into(),
                    Tool {
                        binary: Path::new("/tools").join(name),
                        repository: Path::new("/workspace").join(name),
                        probe: vec!["--version".into()],
                    },
                )
            })
            .collect(),
            require_latest: true,
            skills_manifest: Some("/workspace/lab/.lab/latest/skills.json".into()),
        }
    }

    #[test]
    fn accepts_up_to_eight_agents() {
        let mut config = valid_config();
        for count in [1, 3, 8] {
            config.max_agents = count;
            config.validate().unwrap();
        }
    }

    #[test]
    fn rejects_concurrency_outside_one_to_eight() {
        let mut config = valid_config();
        for count in [0, 9] {
            config.max_agents = count;
            assert!(config.validate().is_err(), "accepted {count} agents");
        }
    }

    #[test]
    fn eight_agents_preserve_daily_and_task_budget_limits() {
        let config = valid_config();
        config.validate().unwrap();
        for daily_seconds in [0, 43201] {
            let invalid = Config {
                daily_seconds,
                ..config.clone()
            };
            assert!(invalid.validate().is_err());
        }
        for task_timeout_seconds in [0, 43201] {
            let invalid = Config {
                task_timeout_seconds,
                ..config.clone()
            };
            assert!(invalid.validate().is_err());
        }
        let mut shorter_day = Config {
            daily_seconds: 120,
            task_timeout_seconds: 120,
            ..config
        };
        shorter_day.validate().unwrap();
        shorter_day.task_timeout_seconds = 121;
        assert!(shorter_day.validate().is_err());
    }
}
