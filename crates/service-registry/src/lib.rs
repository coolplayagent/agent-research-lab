//! Host-owned service configuration persistence and health probes.
mod binding_flow;
pub use binding_flow::binding_failure_code;
mod bindings;
mod knowledge_migration;
mod local_knowledge;
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
    managed: tokio::sync::Mutex<managed::Managed>,
    knowledge: tokio::sync::Mutex<local_knowledge::ManagedKnowledge>,
}
impl Registry {
    pub fn open(state: &Path) -> Result<Self> {
        let state = std::fs::canonicalize(state)?;
        let path = state.join("private/services.json");
        let mut config: Configuration = if path.exists() {
            storage::read(&path)?
        } else {
            Configuration::defaults(&state)
        };
        config.validate()?;
        knowledge_migration::migrate(&path, &mut config)?;
        let active_storage = config
            .storage_id
            .as_ref()
            .map(|id| {
                config
                    .service(id, ServiceKind::Storage)
                    .map(|s| (id.clone(), s.endpoint.clone()))
            })
            .transpose()?;
        managed::validate_locations(&config, &state)?;
        local_knowledge::validate_locations(&config, &state)?;
        Ok(Self {
            path,
            config: Mutex::new(config.clone()),
            bindings: bindings::Operations::open(&state, &config)?,
            managed: tokio::sync::Mutex::new(managed::Managed::default()),
            knowledge: tokio::sync::Mutex::new(local_knowledge::ManagedKnowledge::default()),
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
        for adapter in &next.adapters {
            if adapter.agent != contracts::CodingAgent::Custom && adapter.enabled {
                ensure!(
                    current.adapters.iter().any(|old| old.id == adapter.id
                        && old.enabled
                        && serde_json::to_value(old).ok() == serde_json::to_value(adapter).ok()),
                    ConfigurationFailure(
                        contracts::ServiceConfigurationErrorCode::ValidationRequired
                    )
                );
            }
        }
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
        managed::validate_locations(
            &next,
            self.path
                .parent()
                .and_then(|p| p.parent())
                .context("registry state is missing")?,
        )?;
        local_knowledge::validate_locations(&next, self.path.parent().unwrap().parent().unwrap())?;
        let mut current = self.config.lock().unwrap();
        ensure!(
            next.revision == current.revision,
            binding_flow::Failure(contracts::BindingErrorCode::RevisionConflict)
        );
        for service in &next.services {
            if let Some(old) = current.services.iter().find(|s| s.id == service.id) {
                ensure!(
                    old.kind == service.kind,
                    ConfigurationFailure(contracts::ServiceConfigurationErrorCode::KindImmutable)
                );
            }
        }
        for service in &next.outcome_services {
            if let Some(old) = current.outcome_services.iter().find(|s| s.id == service.id) {
                ensure!(
                    old.kind == service.kind && old.management == service.management,
                    "knowledge_service_role_immutable"
                );
            }
        }
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
    pub async fn prepare_execution(&self, config: &Configuration, executor_id: &str) -> Result<()> {
        let state = self
            .path
            .parent()
            .and_then(|p| p.parent())
            .context("registry state is missing")?;
        self.managed
            .lock()
            .await
            .prepare(config, state, executor_id)
            .await
    }
    pub async fn prepare_local_execution(&self) -> Result<()> {
        let config = self.configuration();
        for service in config.services.iter().filter(|s| {
            s.enabled
                && s.kind == ServiceKind::Executor
                && s.management == contracts::ServiceManagement::LocalManaged
        }) {
            self.prepare_execution(&config, &service.id).await?;
        }
        Ok(())
    }
    pub async fn supervise(&self, mut shutdown: tokio::sync::watch::Receiver<bool>) {
        let config = self.configuration();
        for service in config.outcome_services.iter().filter(|s| s.enabled) {
            let _ = self.prepare_knowledge(service).await;
        }
        for adapter in config.adapters.iter().filter(|a| a.enabled) {
            let _ = self.prepare_execution(&config, &adapter.executor_id).await;
        }
        loop {
            tokio::select! {
                _ = shutdown.changed() => break,
                _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {
                    self.managed.lock().await.maintain(&self.configuration());
                    self.knowledge.lock().await.maintain(&self.configuration());
                }
            }
        }
        *self.managed.lock().await = managed::Managed::default();
        *self.knowledge.lock().await = local_knowledge::ManagedKnowledge::default();
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
        let started = self.managed.lock().await.started.clone();
        let mut probes = tokio::task::JoinSet::new();
        for service in config.services.clone() {
            let started = started.contains(&service.id);
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
                serde_json::json!({"id":service.id,"status":status,"info":info,"error":error,"started":started})
            });
        }
        let mut health = Vec::new();
        while let Some(result) = probes.join_next().await {
            health.push(result?);
        }
        health.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
        let mut outcome_health = Vec::new();
        while let Some(result) = outcome_probes.join_next().await {
            outcome_health.push(result?);
        }
        outcome_health.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
        Ok(
            serde_json::json!({"configuration":config,"health":health,"outcome_health":outcome_health,"restart_required":pending!=self.active_storage,"active_storage":self.active_storage,"protocol_version":rpc::VERSION}),
        )
    }
}
#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
