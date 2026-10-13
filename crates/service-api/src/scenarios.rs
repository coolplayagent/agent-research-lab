//! Replaceable scenario services use the same bounded, owner-checked transport.
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

impl ScenarioRegistration {
    pub fn validate(&self) -> Result<()> {
        crystal::id(&self.id)?;
        rpc::endpoint(&self.endpoint)?;
        ensure!(
            self.protocol_version == crystal::SCENARIO_PROTOCOL_VERSION,
            "unsupported scenario protocol version"
        );
        ensure!(
            !self.capabilities.is_empty()
                && self.capabilities.len() <= ScenarioCapability::ALL.len()
                && self
                    .capabilities
                    .iter()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
                    == self.capabilities.len(),
            "scenario capabilities must be distinct and nonempty"
        );
        ensure!(
            self.capabilities.contains(&ScenarioCapability::Execute),
            "scenario must support execution"
        );
        ensure!(
            self.dependencies.len() <= 16
                && self
                    .dependencies
                    .iter()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
                    == self.dependencies.len(),
            "scenario dependencies exceed bounds or repeat"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioHandshake {
    pub protocol_version: u32,
    pub id: String,
    pub capabilities: Vec<ScenarioCapability>,
    pub ready: bool,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum ScenarioRequest {
    Probe,
    Validate {
        goal: Box<crystal::GoalInput>,
    },
    /// The request identity is stable for one run revision. External side effects
    /// must use that identity durably and reconcile uncertain outcomes on retry.
    Reconcile {
        request_id: String,
        run: Box<crystal::EvolutionRun>,
        goal: Box<crystal::Goal>,
    },
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioValidation {
    pub goal_sha256: String,
    pub valid: bool,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioReconciliation {
    pub request_id: String,
    pub run_id: String,
    pub revision: u64,
    pub goal_revision: u64,
    pub goal_attempts: BTreeMap<String, u32>,
    pub change: Option<crystal::EvolutionChange>,
}

pub async fn probe_scenario(registration: &ScenarioRegistration) -> Result<ScenarioHandshake> {
    ensure!(registration.enabled, "scenario is disabled");
    let info: ScenarioHandshake = rpc::call(
        &registration.endpoint,
        &ScenarioRequest::Probe,
        Duration::from_secs(5),
    )
    .await?;
    ensure!(
        info.protocol_version == registration.protocol_version && info.id == registration.id,
        "scenario identity or protocol mismatch"
    );
    ensure!(
        registration
            .capabilities
            .iter()
            .all(|capability| info.capabilities.contains(capability)),
        "scenario capability mismatch"
    );
    ensure!(info.ready, "scenario is not ready");
    Ok(info)
}
