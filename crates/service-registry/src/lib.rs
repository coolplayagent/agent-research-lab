//! Host-owned service configuration persistence and health probes.
mod binding_flow;
pub use binding_flow::binding_failure_code;
mod bindings;
mod managed;
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
    bindings: bindings::Operations,
    managed: Mutex<managed::Managed>,
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
        #[allow(unused_mut)]
        let mut managed = managed::Managed::default();
        #[cfg(windows)]
        if config
            .services
            .iter()
            .any(|s| s.enabled && s.kind == ServiceKind::Executor)
        {
            // Optional execution failure must not prevent core collaboration startup.
            let _ = managed.prepare(&mut config.clone(), &state);
        }
        Ok(Self {
            path,
            config: Mutex::new(config.clone()),
            bindings: bindings::Operations::open(&state, &config)?,
            managed: Mutex::new(managed),
            active_storage,
        })
    }
    pub fn configuration(&self) -> Configuration {
        self.config.lock().unwrap().clone()
    }
    pub fn save_settings(&self, mut next: Configuration) -> Result<Configuration> {
        let current = self.configuration();
        ensure!(
            next.bindings
                .iter()
                .all(|(person, adapter)| current.bindings.get(person) == Some(adapter)),
            binding_flow::Failure(contracts::BindingErrorCode::ValidationRequired)
        );
        // Evidence is server-owned. Clients cannot forge validation by posting it.
        next.binding_evidence = current
            .binding_evidence
            .into_iter()
            .filter(|(person, evidence)| {
                next.bindings
                    .get(person)
                    .and_then(|id| next.adapters.iter().find(|a| &a.id == id))
                    .is_some_and(|adapter| {
                        serde_json::to_vec(adapter)
                            .is_ok_and(|bytes| storage::digest(&bytes) == evidence.adapter_digest)
                    })
            })
            .collect();
        self.save(next)
    }
    pub fn save(&self, mut next: Configuration) -> Result<Configuration> {
        next.validate()?;
        let mut current = self.config.lock().unwrap();
        ensure!(
            next.revision == current.revision,
            binding_flow::Failure(contracts::BindingErrorCode::RevisionConflict)
        );
        next.revision = next
            .revision
            .checked_add(1)
            .context("configuration revision exhausted")?;
        next.binding_evidence.retain(|person, evidence| {
            next.bindings
                .get(person)
                .and_then(|id| next.adapters.iter().find(|a| &a.id == id))
                .is_some_and(|adapter| {
                    serde_json::to_vec(adapter)
                        .is_ok_and(|bytes| storage::digest(&bytes) == evidence.adapter_digest)
                })
        });
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
#[path = "lib_tests.rs"]
mod tests;
