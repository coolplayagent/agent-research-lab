//! Presentation-neutral readiness of the locally packaged capabilities.
use crate::Registry;
use anyhow::Result;
use contracts::{
    LocalRuntimeComponent as Component, LocalRuntimeErrorCode as Code, LocalRuntimeState as State,
    ServiceHealth as Health, ServiceKind, ServiceManagement,
};
use serde_json::Value;
use service_api::{Configuration, LocalRuntimeStatus};
use std::path::PathBuf;

pub(super) fn binary(name: &str) -> Result<PathBuf> {
    Ok(std::env::current_exe()?.with_file_name(if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.into()
    }))
}
fn health(rows: &[Value], id: &str) -> Option<Health> {
    rows.iter()
        .find(|h| h["id"] == id)
        .and_then(|h| Health::from_value(&h["status"]))
}
fn local_state(packaged: bool, ready: bool, attempted: bool) -> State {
    if ready {
        State::Ready
    } else if !packaged {
        State::Missing
    } else if attempted {
        State::Failed
    } else {
        State::Standby
    }
}
impl Registry {
    pub(super) async fn runtime_status(
        &self,
        config: &Configuration,
        probes: &[Value],
        knowledge_probes: &[Value],
    ) -> Result<Vec<LocalRuntimeStatus>> {
        let storage_package = binary("outcome-hub-storage")?.is_file();
        let storage = self.storage.lock().await;
        let active = storage.active_service().or_else(|| {
            self.active_storage
                .as_ref()
                .and_then(|(id, _)| config.services.iter().find(|s| &s.id == id))
        });
        let storage_health = if let Some((_, endpoint)) = &self.active_storage {
            Some(
                if service_api::probe(endpoint, ServiceKind::Storage)
                    .await
                    .is_ok()
                {
                    Health::Healthy
                } else {
                    Health::Unavailable
                },
            )
        } else {
            None
        };
        let storage_state = match active {
            Some(service) if service.management == ServiceManagement::LocalManaged => local_state(
                storage_package,
                storage_health == Some(Health::Healthy),
                true,
            ),
            _ if self.active_storage.is_some() => State::External,
            _ => State::Development,
        };
        let storage_error = storage
            .error
            .or((storage_state == State::Missing).then_some(Code::PackageMissing));
        drop(storage);
        let execution_package =
            binary("outcome-hub-services")?.is_file() && binary("outcome-hub-agent")?.is_file();
        let executors: Vec<_> = config
            .services
            .iter()
            .filter(|s| s.kind == ServiceKind::Executor && s.enabled)
            .collect();
        let locals: Vec<_> = executors
            .iter()
            .filter(|s| s.management == ServiceManagement::LocalManaged)
            .collect();
        let managed = self.managed.lock().await;
        let ready = !locals.is_empty()
            && locals.iter().all(|s| {
                health(probes, &s.id) == Some(Health::Healthy)
                    && s.sandbox_id
                        .as_ref()
                        .is_some_and(|id| health(probes, id) == Some(Health::Healthy))
            });
        let attempted =
            locals.iter().any(|s| managed.started.contains(&s.id)) || managed.error.is_some();
        let execution_state = if executors.is_empty() {
            State::Disabled
        } else if locals.is_empty() {
            State::External
        } else {
            local_state(execution_package, ready, attempted)
        };
        let execution_error = if execution_state == State::Missing {
            Some(Code::PackageMissing)
        } else if execution_state == State::Failed {
            managed.error.or(Some(Code::StartupFailed))
        } else {
            None
        };
        drop(managed);
        let knowledge_package = binary("outcome-hub-knowledge")?.is_file();
        let knowledge = config
            .outcome_services
            .iter()
            .find(|s| s.management == ServiceManagement::LocalManaged);
        let knowledge_health = knowledge.and_then(|s| health(knowledge_probes, &s.id));
        let knowledge_error = self.knowledge.lock().await.error;
        let knowledge_state = if !cfg!(any(target_os = "linux", windows)) {
            State::Unsupported
        } else if knowledge.is_some_and(|s| !s.enabled) {
            State::Disabled
        } else {
            local_state(
                knowledge_package,
                knowledge_health == Some(Health::Healthy),
                knowledge_error.is_some(),
            )
        };
        Ok(vec![
            LocalRuntimeStatus {
                component: Component::Storage,
                state: storage_state,
                packaged: storage_package,
                can_prepare: false,
                error: storage_error,
                health: storage_health,
            },
            LocalRuntimeStatus {
                component: Component::Execution,
                state: execution_state,
                packaged: execution_package,
                can_prepare: !locals.is_empty() && execution_package,
                error: execution_error,
                health: if ready {
                    Some(Health::Healthy)
                } else if execution_state == State::External {
                    executors.first().and_then(|s| health(probes, &s.id))
                } else {
                    None
                },
            },
            LocalRuntimeStatus {
                component: Component::Knowledge,
                state: knowledge_state,
                packaged: knowledge_package,
                can_prepare: false,
                error: match knowledge_state {
                    State::Unsupported => Some(Code::PlatformUnsupported),
                    State::Missing => Some(Code::PackageMissing),
                    State::Failed => knowledge_error,
                    _ => None,
                },
                health: knowledge_health,
            },
        ])
    }
}

#[cfg(test)]
#[path = "runtime_status_tests.rs"]
mod tests;
