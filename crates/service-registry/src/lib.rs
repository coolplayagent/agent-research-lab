//! Host-owned service configuration persistence and health probes.
use anyhow::{Context, Result, ensure};
use contracts::{ServiceHealth, ServiceKind};
use service_api::*;
use service_protocol as rpc;
use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

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
        let mut outcome_probes = tokio::task::JoinSet::new();
        for service in config.outcome_services.clone() {
            outcome_probes.spawn(async move {
                let (status, error) = if !service.enabled {
                    (ServiceHealth::Disabled, None)
                } else {
                    match probe_outcome_service(&service).await {
                        Ok(_) => (ServiceHealth::Healthy, None),
                        Err(error) => (ServiceHealth::Unavailable, Some(error.to_string())),
                    }
                };
                serde_json::json!({"id":service.id,"status":status,"error":error})
            });
        }
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
        for scenario in config.scenarios.clone() {
            probes.spawn(async move {
                let (status, info, error) = if !scenario.enabled {
                    (ServiceHealth::Disabled, None, None)
                } else {
                    match probe_scenario(&scenario).await {
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
                serde_json::json!({"id":scenario.id,"status":status,"info":info,"error":error})
            });
        }
        let mut scenario_health = Vec::new();
        while let Some(result) = probes.join_next().await {
            scenario_health.push(result?);
        }
        scenario_health.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
        let mut outcome_health = Vec::new();
        while let Some(result) = outcome_probes.join_next().await {
            outcome_health.push(result?);
        }
        outcome_health.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
        Ok(
            serde_json::json!({"configuration":config,"health":health,"scenario_health":scenario_health,"outcome_health":outcome_health,"restart_required":pending!=self.active_storage,"active_storage":self.active_storage,"protocol_version":rpc::VERSION}),
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
    #[test]
    fn scenario_configuration_is_optional_versioned_and_dependency_checked() {
        let root = tempfile::tempdir().unwrap();
        let mut config = Configuration::defaults(root.path());
        let mut legacy = serde_json::to_value(&config).unwrap();
        legacy.as_object_mut().unwrap().remove("scenarios");
        assert!(
            serde_json::from_value::<Configuration>(legacy)
                .unwrap()
                .scenarios
                .is_empty()
        );
        config.scenarios.push(ScenarioRegistration {
            id: "planning".into(),
            endpoint: root.path().join("private/planning.sock"),
            enabled: true,
            protocol_version: 1,
            capabilities: vec![contracts::ScenarioCapability::Execute],
            dependencies: vec!["executor".into()],
        });
        config.validate().unwrap();
        config.scenarios[0].protocol_version = 2;
        assert!(config.validate().is_err());
        config.scenarios[0].protocol_version = 1;
        config.scenarios[0].dependencies = vec!["missing".into()];
        assert!(config.validate().is_err());
        config.scenarios[0].dependencies = vec![];
        config.scenarios.push(config.scenarios[0].clone());
        assert!(config.validate().is_err());
    }
}
