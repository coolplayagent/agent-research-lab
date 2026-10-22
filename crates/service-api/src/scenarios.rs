//! Read-only shape for detecting retired scenario registrations in private configuration.
use crate::*;
use contracts::ScenarioCapability;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioRegistration {
    pub id: String,
    pub endpoint: PathBuf,
    pub enabled: bool,
    pub protocol_version: u32,
    pub capabilities: Vec<ScenarioCapability>,
    #[serde(default)]
    pub dependencies: Vec<String>,
}
