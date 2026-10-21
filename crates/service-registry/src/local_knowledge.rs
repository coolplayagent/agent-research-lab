//! Lifecycle of the optional packaged knowledge gateway; target data stays in that service.
use crate::{Registry, managed::child::Child};
use anyhow::{Context, Result, ensure};
use contracts::{OutcomeKind, OutcomeServiceKind, ServiceManagement};
use service_api::{Configuration, OutcomeService};
use std::{
    path::Path,
    time::{Duration, Instant},
};

#[derive(Default)]
pub(super) struct ManagedKnowledge {
    child: Option<(OutcomeService, Child)>,
}
pub(super) fn validate_locations(config: &Configuration, state: &Path) -> Result<()> {
    for service in &config.outcome_services {
        ensure!(
            service.kind != OutcomeServiceKind::Knowledge
                || service.management == ServiceManagement::LocalManaged,
            contracts::KnowledgeError::new(contracts::KnowledgeErrorCode::ExternalServiceRemoved)
        );
        if service.management == ServiceManagement::LocalManaged {
            ensure!(
                !config.services.iter().any(|s| s.id == service.id),
                "knowledge_managed_id_conflict"
            );
            ensure!(
                service.endpoint == state.join(format!("private/services/{}.sock", service.id)),
                "knowledge_managed_endpoint_required"
            );
        }
    }
    Ok(())
}
impl ManagedKnowledge {
    pub fn maintain(&mut self, config: &Configuration) {
        if self.child.as_mut().is_some_and(|(service, child)| {
            !config.outcome_services.iter().any(|s| {
                s.enabled && serde_json::to_vec(s).ok() == serde_json::to_vec(service).ok()
            }) || !child.running()
        }) {
            self.child = None;
        }
    }
    async fn prepare(&mut self, service: &OutcomeService, state: &Path) -> Result<()> {
        ensure!(
            service.kind != OutcomeServiceKind::Knowledge
                || service.management == ServiceManagement::LocalManaged,
            contracts::KnowledgeError::new(contracts::KnowledgeErrorCode::ExternalServiceRemoved)
        );
        if service.management != ServiceManagement::LocalManaged {
            return Ok(());
        }
        ensure!(
            cfg!(target_os = "linux"),
            "knowledge_local_platform_unsupported"
        );
        ensure!(
            service.enabled && service.kind == OutcomeServiceKind::Knowledge,
            "knowledge_service_unavailable"
        );
        if self.child.as_ref().is_some_and(|(old, _)| {
            serde_json::to_vec(old).ok() != serde_json::to_vec(service).ok()
        }) {
            self.child = None;
        }
        if self
            .child
            .as_mut()
            .is_some_and(|(_, child)| !child.running())
        {
            self.child = None;
        }
        if self.child.is_none() {
            ensure!(
                service_api::probe_outcome_service(service).await.is_err(),
                "knowledge_managed_endpoint_in_use"
            );
            let binary = std::env::current_exe()?.with_file_name("outcome-hub-knowledge");
            ensure!(binary.is_file(), "knowledge_runtime_missing");
            let directory = state.join(format!("private/managed-{}", service.id));
            let args = vec![
                "--id".into(),
                service.id.clone(),
                "--state-dir".into(),
                directory.to_string_lossy().into_owned(),
                "--socket".into(),
                service.endpoint.to_string_lossy().into_owned(),
                "--max-seconds".into(),
                "43200".into(),
            ];
            self.child = Some((
                service.clone(),
                Child::spawn_with(&binary, &directory, args)?,
            ));
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if service_api::probe_outcome_service(service).await.is_ok() {
                return Ok(());
            }
            if Instant::now() >= deadline {
                self.child = None;
                anyhow::bail!("knowledge_runtime_unavailable");
            }
            ensure!(
                self.child
                    .as_mut()
                    .is_some_and(|(_, child)| child.running()),
                "knowledge_runtime_unavailable"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}
impl Registry {
    pub async fn prepare_knowledge(&self, service: &OutcomeService) -> Result<()> {
        let state = self
            .path
            .parent()
            .and_then(|p| p.parent())
            .context("registry state is missing")?;
        self.knowledge.lock().await.prepare(service, state).await
    }
    pub async fn local_knowledge(&self, revision: u64) -> Result<OutcomeService> {
        let mut managed = self.knowledge.lock().await;
        let mut config = self.configuration();
        ensure!(
            config.revision == revision,
            "knowledge_configuration_conflict"
        );
        let state = self
            .path
            .parent()
            .and_then(|p| p.parent())
            .context("registry state is missing")?;
        if let Some(service) = config.outcome_services.iter().find(|s| {
            s.management == ServiceManagement::LocalManaged
                && s.kind == OutcomeServiceKind::Knowledge
        }) {
            managed.prepare(service, state).await?;
            return Ok(service.clone());
        }
        let id = (1..=64)
            .map(|n| {
                if n == 1 {
                    "local-knowledge".to_owned()
                } else {
                    format!("local-knowledge-{n}")
                }
            })
            .find(|id| {
                !config.outcome_services.iter().any(|s| s.id == *id)
                    && !config.services.iter().any(|s| s.id == *id)
            })
            .context("knowledge_service_limit")?;
        let service = OutcomeService {
            endpoint: state.join(format!("private/services/{id}.sock")),
            id,
            kind: OutcomeServiceKind::Knowledge,
            enabled: true,
            management: ServiceManagement::LocalManaged,
            protocol_version: 1,
            outcome_kinds: OutcomeKind::ALL.to_vec(),
            dependencies: vec![],
            max_seconds: 120,
        };
        managed.prepare(&service, state).await?;
        config.outcome_services.push(service.clone());
        if let Err(error) = self.save(config) {
            managed.child = None;
            return Err(error);
        }
        Ok(service)
    }
}
#[cfg(test)]
#[path = "local_knowledge_tests.rs"]
mod tests;
