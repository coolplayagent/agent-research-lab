//! Owned execution processes. External endpoints are only probed, never replaced.
pub(super) mod child;
use anyhow::{Context, Result, ensure};
use contracts::{ServiceKind, ServiceManagement};
use service_api::{Configuration, Service};
use std::{
    path::Path,
    time::{Duration, Instant},
};
#[derive(Default)]
pub(super) struct Managed {
    children: Vec<(Service, child::Child)>,
    pub started: std::collections::BTreeSet<String>,
}
pub(super) fn validate_locations(config: &Configuration, state: &Path) -> Result<()> {
    for service in &config.services {
        if service.management == ServiceManagement::LocalManaged {
            ensure!(
                service.endpoint == state.join(format!("private/services/{}.sock", service.id)),
                "managed service endpoint must be inside its private state directory"
            );
        }
    }
    Ok(())
}
impl Managed {
    pub fn maintain(&mut self, config: &Configuration) {
        self.children.retain_mut(|(service, child)| {
            config.services.iter().any(|current| {
                current.enabled
                    && serde_json::to_value(current).ok() == serde_json::to_value(&*service).ok()
            }) && child.running()
        });
    }
    pub async fn prepare(
        &mut self,
        config: &Configuration,
        state: &Path,
        executor_id: &str,
    ) -> Result<()> {
        let binary = std::env::current_exe()?.with_file_name(if cfg!(windows) {
            "outcome-hub-services.exe"
        } else {
            "outcome-hub-services"
        });
        self.prepare_with(config, state, executor_id, &binary).await
    }
    async fn prepare_with(
        &mut self,
        config: &Configuration,
        state: &Path,
        executor_id: &str,
        binary: &Path,
    ) -> Result<()> {
        validate_locations(config, state)?;
        self.maintain(config);
        let executor = config.service(executor_id, ServiceKind::Executor)?;
        let sandbox = config.service(
            executor
                .sandbox_id
                .as_deref()
                .context("executor sandbox is missing")?,
            ServiceKind::Sandbox,
        )?;
        for service in [sandbox, executor] {
            if service.management == ServiceManagement::LocalManaged
                && !self
                    .children
                    .iter()
                    .any(|(owned, _)| owned.id == service.id)
            {
                self.started.insert(service.id.clone());
                ensure!(binary.is_file(), "packaged execution service is missing");
                let child = child::Child::spawn(binary, service, state)?;
                self.children.push((service.clone(), child));
            }
            let deadline = Instant::now() + Duration::from_secs(15);
            loop {
                match service_api::probe(&service.endpoint, service.kind).await {
                    Ok(_) => break,
                    Err(error) => {
                        if service.management == ServiceManagement::External
                            || Instant::now() >= deadline
                        {
                            return Err(error.context("execution runtime is not ready"));
                        }
                        if let Some((_, child)) =
                            self.children.iter_mut().find(|(s, _)| s.id == service.id)
                        {
                            ensure!(child.running(), "managed service exited before readiness");
                        }
                    }
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
        Ok(())
    }
}
#[cfg(test)]
#[path = "managed_tests.rs"]
mod tests;
