//! Frozen activation policy, input and installation proof records.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Policy {
    pub(super) schema_version: u32,
    pub(super) service_config: PathBuf,
    pub(super) main_unit: String,
    pub(super) health_url: String,
    pub(super) repositories: BTreeMap<String, Repository>,
    #[serde(default)]
    pub(super) storage_state_dir: Option<PathBuf>,
    #[serde(default)]
    pub(super) storage_socket: Option<PathBuf>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Repository {
    pub(super) checkout: PathBuf,
    pub(super) build_commands: Vec<Vec<String>>,
    #[serde(default)]
    pub(super) package_directory: PathBuf,
    #[serde(default)]
    pub(super) binary_relative_path: PathBuf,
    #[serde(default)]
    pub(super) probe: Vec<String>,
    #[serde(default)]
    pub(super) platform_units: Vec<PlatformUnit>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "software_verification::optional_budget"
    )]
    pub(super) qualitygate_snapshot_max_file_mib: Option<software_verification::SnapshotMaxFileMib>,
}

impl Repository {
    pub(super) fn qualitygate_command(
        &self,
        program: String,
        source: &Path,
        merged: &str,
    ) -> Vec<String> {
        software_verification::qualitygate_command(
            program,
            source,
            format!("{merged}^..{merged}"),
            self.qualitygate_snapshot_max_file_mib,
        )
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PlatformUnit {
    pub(super) unit: String,
    pub(super) artifact: PathBuf,
    pub(super) unit_file: PathBuf,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Request {
    pub(super) schema_version: u32,
    pub(super) iteration_id: String,
    pub(super) goal_id: String,
    pub(super) candidate_commit: String,
    pub(super) merged_commit: String,
    pub(super) repository: String,
    pub(super) merge: Value,
    pub(super) evidence: delivery::ReleaseEvidence,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Prepared {
    pub(super) request: Request,
    pub(super) policy: Policy,
    pub(super) repository: Repository,
    pub(super) state_dir: PathBuf,
    pub(super) config_sha256: String,
    pub(super) policy_sha256: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SwitchProof {
    pub(super) previous_config_sha256: String,
    pub(super) next_config_sha256: String,
    pub(super) main_artifact_sha256: String,
    pub(super) artifact: PathBuf,
    pub(super) artifact_sha256: String,
    pub(super) units: Vec<UnitProof>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct UnitProof {
    pub(super) unit: String,
    pub(super) unit_file: PathBuf,
    pub(super) previous_unit_sha256: String,
    pub(super) next_unit_sha256: String,
    pub(super) previous_artifact_sha256: String,
    pub(super) next_artifact_sha256: String,
}

#[cfg(test)]
#[path = "model_tests.rs"]
mod tests;
