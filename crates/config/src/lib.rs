use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
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
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub agent_backends: BTreeMap<String, BackendSpec>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub role_backends: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multi_agent: Option<Settings>,
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
        validate_backends(self)?;
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

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capabilities {
    pub structured_result: bool,
    pub read_workspace: bool,
    pub write_workspace: bool,
    pub tool_execution: bool,
    pub desktop: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum BackendSpec {
    Codex {
        program: String,
    },
    JsonProcess {
        program: PathBuf,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        args: Vec<String>,
        capabilities: Capabilities,
        /// Names only. Values never enter frozen requests, receipts or config.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        env_allowlist: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub id: String,
    pub spec: BackendSpec,
    pub executable: PathBuf,
    pub executable_sha256: String,
}

impl BackendSpec {
    pub fn capabilities(&self) -> Capabilities {
        match self {
            Self::Codex { .. } => Capabilities {
                structured_result: true,
                read_workspace: true,
                write_workspace: true,
                tool_execution: true,
                desktop: true,
            },
            Self::JsonProcess { capabilities, .. } => capabilities.clone(),
        }
    }
    pub fn program(&self) -> &str {
        match self {
            Self::Codex { program } => program,
            Self::JsonProcess { program, .. } => program.to_str().unwrap_or(""),
        }
    }
}
/// Validate declarations without spawning adapters or probing model providers.
pub fn validate_backends(c: &Config) -> Result<()> {
    for (id, spec) in &c.agent_backends {
        crate::safe_id(id)?;
        ensure!(id != "codex", "codex is the reserved legacy backend");
        ensure!(
            !spec.program().is_empty(),
            "backend program is empty or not UTF-8"
        );
        if let BackendSpec::JsonProcess {
            program,
            args,
            env_allowlist,
            ..
        } = spec
        {
            ensure!(
                program.is_absolute(),
                "JSON bridge executable must be absolute"
            );
            ensure!(
                args.len() <= 64 && args.iter().all(|a| a.len() <= 4096 && !a.contains('\0')),
                "invalid bridge arguments"
            );
            let mut seen = BTreeSet::new();
            ensure!(
                env_allowlist.len() <= 16,
                "too many backend credential variables"
            );
            for name in env_allowlist {
                agent_policy::validate_environment_name(name)?;
                ensure!(seen.insert(name), "duplicate backend environment variable");
            }
        }
    }
    for (role, id) in &c.role_backends {
        ensure!(
            c.models.contains_key(role),
            "backend role has no model binding: {role}"
        );
        ensure!(
            id == "codex" || c.agent_backends.contains_key(id),
            "unknown backend: {id}"
        );
    }
    Ok(())
}

/// Optional host configuration. Its absence preserves historical config digests.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub shared_research: bool,
}

/// Membership is part of the immutable task, never an agent report field.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeamBinding {
    pub id: String,
    /// Optional host-assigned bounded cell within a larger research team.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cell: Option<String>,
    #[serde(default)]
    pub topics: Vec<String>,
    #[serde(default)]
    pub query: String,
}
impl TeamBinding {
    pub fn validate(&self) -> Result<()> {
        crate::safe_id(&self.id)?;
        if let Some(cell) = &self.cell {
            crate::safe_id(cell)?;
        }
        ensure!(self.topics.len() <= 8, "at most eight communication topics");
        let mut unique = BTreeSet::new();
        for topic in &self.topics {
            crate::safe_id(topic)?;
            ensure!(
                topic.len() <= 32 && unique.insert(topic),
                "invalid or duplicate communication topic"
            );
        }
        ensure!(
            self.query.len() <= 256,
            "communication query exceeds 256 bytes"
        );
        Ok(())
    }
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
            agent_backends: Default::default(),
            multi_agent: None,
            role_backends: Default::default(),
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
