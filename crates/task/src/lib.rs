//! Frozen task and attempt records shared by host orchestration crates.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

fn is_false(value: &bool) -> bool {
    !*value
}
fn is_zero(value: &u8) -> bool {
    *value == 0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Task {
    /// Stable digital person chosen by the host; absent legacy tasks remain compatible.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persona_id: Option<String>,
    pub id: String,
    pub role: String,
    pub repository: String,
    pub prompt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub communication: Option<config::TeamBinding>,
    /// A frozen invocation ceiling; omitted historical tasks retain three attempts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_attempts: Option<u8>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub use_memory: bool,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub depth: u8,
    #[serde(default)]
    pub write: bool,
    #[serde(default)]
    pub exploratory: bool,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub required_tools: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    /// Frozen identity and bounded relay-memory context for this experiment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persona: Option<PersonaSnapshot>,
    pub task: Task,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend: Option<config::Binding>,
    pub source_commit: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline_commit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub research_inputs: Option<inputs::ResearchInputs>,
    pub superpod_commit: String,
    pub prompt_digest: String,
    pub config_digest: String,
    pub worktree: PathBuf,
    pub run_id: String,
    pub attempt: u32,
    pub last_error: Option<String>,
    pub launch: Option<Launch>,
    #[serde(default)]
    pub retry_after: Option<i64>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Launch {
    pub pid: u32,
    pub process_start: String,
    pub lease: Value,
    pub attempt: Value,
    pub log_dir: PathBuf,
    #[serde(default)]
    pub tools: Value,
    #[serde(default)]
    pub git_dir: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend_request_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub communication: Option<communication::LaunchContext>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub communication_member: Option<communication::HostMember>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub communication_gap: Option<String>,
}

/// Host-owned identity inputs, immutable across retries and pinned by the prompt digest.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersonaSnapshot {
    pub id: String,
    pub name: String,
    pub revision: u64,
    pub soul: String,
    pub memory: Option<PersonaMemory>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersonaMemory {
    pub context: String,
    pub sha256: String,
    pub pack_sha256: String,
    pub executable_sha256: String,
    pub captured_at: i64,
}
