//! External knowledge service protocol. Configuration is not execution evidence.
use crate::*;
use collaboration_model::{KnowledgeWrite, KnowledgeWriteResult, Outcome};
use contracts::{OutcomeKind, OutcomeServiceKind};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutcomeService {
    pub id: String,
    pub kind: OutcomeServiceKind,
    pub endpoint: PathBuf,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "crate::registry::external_management")]
    pub management: contracts::ServiceManagement,
    pub protocol_version: u32,
    pub outcome_kinds: Vec<OutcomeKind>,
    pub dependencies: Vec<String>,
    pub max_seconds: u64,
}
impl OutcomeService {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.management != contracts::ServiceManagement::LocalManaged
                || self.kind == OutcomeServiceKind::Knowledge,
            "knowledge_managed_kind_required"
        );
        collaboration_model::id(&self.id)?;
        rpc::endpoint(&self.endpoint)?;
        ensure!(
            self.protocol_version == collaboration_model::OUTCOME_PROTOCOL_VERSION,
            "unsupported outcome service protocol"
        );
        ensure!(
            (1..=600).contains(&self.max_seconds),
            "outcome service time limit must be 1..600 seconds"
        );
        ensure!(
            !self.outcome_kinds.is_empty()
                && self.outcome_kinds.len() <= OutcomeKind::ALL.len()
                && self
                    .outcome_kinds
                    .iter()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
                    == self.outcome_kinds.len(),
            "invalid outcome kinds"
        );
        ensure!(
            self.dependencies.len() <= 16
                && self
                    .dependencies
                    .iter()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
                    == self.dependencies.len(),
            "invalid outcome service dependencies"
        );
        Ok(())
    }
    pub fn digest(&self) -> Result<String> {
        Ok(collaboration_model::digest(&serde_json::to_vec(self)?))
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutcomeServicesBinding {
    #[serde(default)]
    pub verifiers: BTreeMap<OutcomeKind, String>,
    pub knowledge: Option<KnowledgeTarget>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeTarget {
    pub service_id: String,
    pub repository: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutcomeServiceHandshake {
    pub protocol_version: u32,
    pub id: String,
    pub kind: OutcomeServiceKind,
    pub outcome_kinds: Vec<OutcomeKind>,
    pub ready: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum OutcomeServiceRequest {
    Probe,
    Knowledge {
        request: knowledge_model::Request,
    },
    /// Idempotent compare-and-swap against expected_revision. Credentials remain in the service.
    Write {
        write: Box<KnowledgeWrite>,
        outcome: Box<Outcome>,
    },
    /// Read-only reconciliation must never repeat a remote write.
    LookupWrite {
        id: String,
        request_digest: String,
    },
}

pub async fn probe_outcome_service(service: &OutcomeService) -> Result<OutcomeServiceHandshake> {
    ensure!(service.enabled, "outcome service is disabled");
    let info: OutcomeServiceHandshake = rpc::call(
        &service.endpoint,
        &OutcomeServiceRequest::Probe,
        Duration::from_secs(5),
    )
    .await?;
    ensure!(
        info.protocol_version == service.protocol_version
            && info.id == service.id
            && info.kind == service.kind,
        "outcome service identity or protocol mismatch"
    );
    ensure!(
        service
            .outcome_kinds
            .iter()
            .all(|kind| info.outcome_kinds.contains(kind)),
        "outcome service capability mismatch"
    );
    ensure!(info.ready, "outcome service is not ready");
    Ok(info)
}
pub async fn write_outcome(
    service: &OutcomeService,
    write: KnowledgeWrite,
    outcome: Outcome,
) -> Result<KnowledgeWriteResult> {
    ensure!(
        service.kind == OutcomeServiceKind::Knowledge
            && service.outcome_kinds.contains(&outcome.input.kind),
        "unsupported knowledge kind"
    );
    ensure!(
        service.digest()? == write.input.service_digest
            && service.id == write.input.service_id
            && outcome.digest == write.input.outcome_digest,
        "outcome knowledge binding changed"
    );
    rpc::call(
        &service.endpoint,
        &OutcomeServiceRequest::Write {
            write: Box::new(write),
            outcome: Box::new(outcome),
        },
        Duration::from_secs(service.max_seconds),
    )
    .await
}
