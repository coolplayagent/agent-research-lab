use super::*;
use anyhow::Context;
use std::path::{Path, PathBuf};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Service {
    pub id: String,
    pub kind: ServiceKind,
    pub endpoint: PathBuf,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "external_management")]
    pub management: contracts::ServiceManagement,
    pub sandbox_id: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Configuration {
    pub revision: u64,
    pub storage_id: Option<String>,
    pub services: Vec<Service>,
    pub adapters: Vec<Adapter>,
    pub bindings: Bindings,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub binding_evidence: BTreeMap<String, crate::BindingEvidence>,
    #[serde(default)]
    pub scenarios: Vec<ScenarioRegistration>,
    #[serde(default)]
    pub outcome_services: Vec<OutcomeService>,
    #[serde(default)]
    pub outcome_bindings: BTreeMap<String, OutcomeServicesBinding>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub knowledge_groups: BTreeMap<String, KnowledgeTarget>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub knowledge_goals: BTreeMap<String, KnowledgeTarget>,
}

#[cfg(test)]
#[path = "registry_tests.rs"]
mod tests;

pub(crate) fn external_management(value: &contracts::ServiceManagement) -> bool {
    *value == contracts::ServiceManagement::External
}

impl Configuration {
    pub fn defaults(state: &Path) -> Self {
        let services = [ServiceKind::Executor, ServiceKind::Sandbox]
            .iter()
            .map(|kind| Service {
                id: kind.to_string(),
                kind: *kind,
                endpoint: state.join(format!("private/services/{kind}.sock")),
                enabled: true,
                management: contracts::ServiceManagement::LocalManaged,
                sandbox_id: (*kind == ServiceKind::Executor).then(|| "sandbox".into()),
            })
            .collect();
        let adapters = [
            CodingAgent::ClaudeCode,
            CodingAgent::Codex,
            CodingAgent::DeepseekHarness,
            CodingAgent::Pi,
        ]
        .into_iter()
        .map(|agent| Adapter {
            id: agent.to_string(),
            agent,
            protocol: AgentProtocol::JsonStdioV1,
            enabled: false,
            executor_id: "executor".into(),
            command: vec![],
            env_names: vec![],
            read_only_paths: vec![],
            network: true,
            max_seconds: 1800,
        })
        .collect();
        Self {
            revision: 0,
            storage_id: None,
            services,
            adapters,
            bindings: Bindings::new(),
            binding_evidence: BTreeMap::new(),
            scenarios: vec![],
            outcome_services: vec![],
            outcome_bindings: BTreeMap::new(),
            knowledge_groups: BTreeMap::new(),
            knowledge_goals: BTreeMap::new(),
        }
    }
    pub fn service(&self, id: &str, kind: ServiceKind) -> Result<&Service> {
        let service = self
            .services
            .iter()
            .find(|s| s.id == id && s.kind == kind)
            .context("service reference has wrong kind or is missing")?;
        ensure!(service.enabled, "referenced service is disabled");
        Ok(service)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.services.len() <= 64 && self.adapters.len() <= 64 && self.bindings.len() <= 20000,
            "service registry exceeds bound"
        );
        ensure!(
            self.scenarios.is_empty(),
            "legacy scenario services must be removed from private/services.json before startup"
        );
        ensure!(
            self.outcome_bindings.is_empty(),
            "legacy topic bindings must be removed from private/services.json before startup"
        );
        ensure!(
            self.outcome_services
                .iter()
                .filter(|s| s.management == contracts::ServiceManagement::LocalManaged)
                .count()
                <= 1,
            "knowledge_local_service_limit"
        );
        let mut ids = std::collections::BTreeSet::new();
        for s in &self.services {
            collaboration_model::id(&s.id)?;
            ensure!(ids.insert(&s.id), "duplicate service ID");
            rpc::endpoint(&s.endpoint)?;
            ensure!(
                s.management != contracts::ServiceManagement::LocalManaged
                    || s.kind != ServiceKind::Storage,
                "storage cannot be locally managed by the execution host"
            );
            ensure!(
                s.kind == ServiceKind::Executor || s.sandbox_id.is_none(),
                "only executors depend on sandboxes"
            );
        }
        if let Some(id) = &self.storage_id {
            self.service(id, ServiceKind::Storage)?;
        }
        for service in self
            .services
            .iter()
            .filter(|s| s.enabled && s.kind == ServiceKind::Executor)
        {
            self.service(
                service
                    .sandbox_id
                    .as_deref()
                    .context("executor requires a sandbox service")?,
                ServiceKind::Sandbox,
            )?;
        }
        ids.clear();
        for adapter in &self.adapters {
            adapter.validate()?;
            ensure!(ids.insert(&adapter.id), "duplicate adapter ID");
            if adapter.enabled {
                self.service(&adapter.executor_id, ServiceKind::Executor)?;
            }
        }
        for (person, adapter) in &self.bindings {
            collaboration_model::id(person)?;
            ensure!(person != "operator", "operator cannot bind an executor");
            ensure!(
                self.adapters.iter().any(|a| a.id == *adapter),
                "binding references missing adapter"
            );
        }
        ensure!(
            self.outcome_services.len() <= 64,
            "outcome registry exceeds bound"
        );
        ids.clear();
        for service in &self.outcome_services {
            ensure!(
                service.kind == contracts::OutcomeServiceKind::Knowledge,
                "legacy verifier services must be removed from private/services.json before startup"
            );
            service.validate()?;
            ensure!(ids.insert(&service.id), "duplicate outcome service ID");
            for dependency in &service.dependencies {
                let target = self
                    .services
                    .iter()
                    .find(|item| item.id == *dependency)
                    .context("outcome dependency missing")?;
                ensure!(
                    !service.enabled || target.enabled,
                    "outcome dependency disabled"
                );
            }
        }
        ensure!(
            self.knowledge_groups.len() <= 2048 && self.knowledge_goals.len() <= 4096,
            "knowledge_binding_limit"
        );
        for (scope, target) in self.knowledge_groups.iter().chain(&self.knowledge_goals) {
            collaboration_model::id(scope)?;
            collaboration_model::id(&target.repository)?;
            ensure!(
                self.outcome_services
                    .iter()
                    .any(|s| s.id == target.service_id
                        && s.kind == contracts::OutcomeServiceKind::Knowledge),
                "invalid knowledge binding"
            );
        }
        Ok(())
    }
}
