use super::*;
use anyhow::Context;
use contracts::ServiceHealth;
use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Service {
    pub id: String,
    pub kind: ServiceKind,
    pub endpoint: PathBuf,
    pub enabled: bool,
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
}
impl Configuration {
    pub fn defaults(state: &Path) -> Self {
        let services = ServiceKind::ALL
            .iter()
            .map(|kind| Service {
                id: kind.to_string(),
                kind: *kind,
                endpoint: state.join(format!("private/services/{kind}.sock")),
                enabled: true,
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
        let mut ids = std::collections::BTreeSet::new();
        for s in &self.services {
            crystal::id(&s.id)?;
            ensure!(ids.insert(&s.id), "duplicate service ID");
            rpc::endpoint(&s.endpoint)?;
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
            crystal::id(person)?;
            ensure!(person != "operator", "operator cannot bind an executor");
            ensure!(
                self.adapters.iter().any(|a| a.id == *adapter),
                "binding references missing adapter"
            );
        }
        Ok(())
    }
    pub fn storage(&self, state: &Path) -> Result<im_storage::Client> {
        match &self.storage_id {
            Some(id) => {
                im_storage::Client::remote(self.service(id, ServiceKind::Storage)?.endpoint.clone())
            }
            None => Ok(crystal::Hub::open(&state.join("crystal"))?.into()),
        }
    }
}
pub struct Registry {
    path: PathBuf,
    config: Mutex<Configuration>,
    active_storage: Option<(String, PathBuf)>,
}
impl Registry {
    pub fn open(state: &Path) -> Result<Self> {
        let state = std::fs::canonicalize(state)?;
        let path = state.join("private/services.json");
        let config: Configuration = if path.exists() {
            storage::read(&path)?
        } else {
            Configuration::defaults(&state)
        };
        config.validate()?;
        let active_storage = config
            .storage_id
            .as_ref()
            .map(|id| {
                config
                    .service(id, ServiceKind::Storage)
                    .map(|s| (id.clone(), s.endpoint.clone()))
            })
            .transpose()?;
        Ok(Self {
            path,
            config: Mutex::new(config),
            active_storage,
        })
    }
    pub fn configuration(&self) -> Configuration {
        self.config.lock().unwrap().clone()
    }
    pub fn save(&self, mut next: Configuration) -> Result<Configuration> {
        next.validate()?;
        let mut current = self.config.lock().unwrap();
        ensure!(
            next.revision == current.revision,
            "service configuration revision conflict"
        );
        next.revision = next
            .revision
            .checked_add(1)
            .context("configuration revision exhausted")?;
        storage::write(&self.path, &next)?;
        *current = next.clone();
        Ok(next)
    }
    pub async fn view(&self) -> Result<serde_json::Value> {
        let config = self.configuration();
        let pending = config
            .storage_id
            .as_ref()
            .map(|id| {
                config
                    .service(id, ServiceKind::Storage)
                    .map(|s| (id.clone(), s.endpoint.clone()))
            })
            .transpose()?;
        let mut probes = tokio::task::JoinSet::new();
        for service in config.services.clone() {
            probes.spawn(async move {
                let (status, info, error) = if !service.enabled {
                    (ServiceHealth::Disabled, None, None)
                } else {
                    match probe(&service.endpoint, service.kind).await {
                        Ok(info) => (ServiceHealth::Healthy, Some(info), None),
                        Err(error) => {
                            let message = error.to_string();
                            let status =
                                if message.contains("mismatch") || message.contains("version") {
                                    ServiceHealth::ProtocolMismatch
                                } else {
                                    ServiceHealth::Unavailable
                                };
                            (status, None, Some(message))
                        }
                    }
                };
                serde_json::json!({"id":service.id,"status":status,"info":info,"error":error})
            });
        }
        let mut health = Vec::new();
        while let Some(result) = probes.join_next().await {
            health.push(result?);
        }
        health.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
        Ok(
            serde_json::json!({"configuration":config,"health":health,"restart_required":pending!=self.active_storage,"active_storage":self.active_storage,"protocol_version":rpc::VERSION}),
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn configuration_is_revision_checked_and_dependencies_are_typed() {
        let root = tempfile::tempdir().unwrap();
        let registry = Registry::open(root.path()).unwrap();
        let initial = registry.configuration();
        let mut next = initial.clone();
        next.storage_id = Some("sandbox".into());
        assert!(registry.save(next).is_err());
        let mut next = initial.clone();
        next.adapters[0].enabled = true;
        assert!(registry.save(next).is_err());
        registry.save(initial.clone()).unwrap();
        assert!(registry.save(initial).is_err());
        assert_eq!(
            Registry::open(root.path())
                .unwrap()
                .configuration()
                .revision,
            1
        );
    }
}
