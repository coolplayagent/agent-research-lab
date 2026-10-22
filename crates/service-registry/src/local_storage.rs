//! Storage has its own lifecycle and never follows execution configuration cleanup.
use crate::{Registry, managed::child::Child};
use anyhow::{Context, Result, ensure};
use contracts::{LocalRuntimeErrorCode as Code, ServiceKind, ServiceManagement};
use serde::{Deserialize, Serialize};
use service_api::{LocalRuntimeFailure as Failure, Service};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

#[derive(Serialize, Deserialize, PartialEq, Eq)]
struct Identity {
    id: String,
    endpoint: PathBuf,
    directory: PathBuf,
}
#[derive(Default)]
pub(super) struct ManagedStorage {
    child: Option<Child>,
    active: Option<(Service, PathBuf, PathBuf)>,
    owner: Option<storage::LockGuard>,
    attempts: u32,
    retry_after: Option<Instant>,
    pub error: Option<Code>,
}
impl ManagedStorage {
    pub fn active_service(&self) -> Option<&Service> {
        self.active.as_ref().map(|(service, _, _)| service)
    }
    async fn launch(&mut self, service: &Service, state: &Path, binary: &Path) -> Result<()> {
        ensure!(binary.is_file(), Failure(Code::PackageMissing));
        ensure!(
            service_protocol::connect(&service.endpoint).await.is_err(),
            Failure(Code::EndpointInUse)
        );
        let directory = state.join(format!("private/managed-{}", service.id));
        let marker = state.join(format!("private/storage-owner-{}.json", service.id));
        let identity = Identity {
            id: service.id.clone(),
            endpoint: service.endpoint.clone(),
            directory: directory.clone(),
        };
        if marker.exists() {
            let old: Identity = storage::read(&marker)?;
            ensure!(
                old == identity
                    && std::fs::symlink_metadata(directory.join("messages.sqlite"))
                        .is_ok_and(|m| m.is_file() && !m.file_type().is_symlink()),
                Failure(Code::DataMissing)
            );
        }
        let args = vec![
            "--state-dir".into(),
            directory.to_string_lossy().into_owned(),
            "--socket".into(),
            service.endpoint.to_string_lossy().into_owned(),
            "--max-seconds".into(),
            "43200".into(),
        ];
        let mut child =
            Child::spawn_with(binary, &directory, args).context(Failure(Code::StartupFailed))?;
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if service_api::probe(&service.endpoint, ServiceKind::Storage)
                .await
                .is_ok()
            {
                storage::write(&marker, &identity)?;
                self.child = Some(child);
                self.error = None;
                return Ok(());
            }
            ensure!(child.running(), Failure(Code::StartupFailed));
            ensure!(Instant::now() < deadline, Failure(Code::ReadinessTimeout));
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
    async fn prepare(&mut self, service: &Service, state: &Path, binary: &Path) -> Result<()> {
        if self.active.is_some() {
            ensure!(
                self.child.as_mut().is_some_and(Child::running),
                Failure(self.error.unwrap_or(Code::StartupFailed))
            );
            return Ok(());
        }
        self.owner = Some(storage::lock(&state.join("private/managed-storage.lock"))?);
        self.launch(service, state, binary).await?;
        self.active = Some((service.clone(), state.into(), binary.into()));
        Ok(())
    }
    pub async fn recover(&mut self) {
        let Some((service, state, binary)) = self.active.clone() else {
            return;
        };
        if self.child.as_mut().is_some_and(Child::running) {
            return;
        }
        self.child = None;
        if self.attempts >= 3 {
            self.error = Some(Code::RecoveryExhausted);
            return;
        }
        if self.retry_after.is_some_and(|next| Instant::now() < next) {
            return;
        }
        self.attempts += 1;
        self.retry_after = Some(Instant::now() + Duration::from_secs(1 << self.attempts));
        if let Err(error) = self.launch(&service, &state, &binary).await {
            self.error = Some(
                error
                    .downcast_ref::<Failure>()
                    .map_or(Code::StartupFailed, |e| e.0),
            );
        }
    }
}
impl Registry {
    /// Called before creating any collaboration client or registering the operator.
    pub async fn prepare_storage(&self) -> Result<()> {
        let config = self.configuration();
        let Some(id) = &config.storage_id else {
            return Ok(());
        };
        let service = config.service(id, ServiceKind::Storage)?;
        if service.management != ServiceManagement::LocalManaged {
            return Ok(());
        }
        let state = self
            .path
            .parent()
            .and_then(Path::parent)
            .context("registry state missing")?;
        let binary = super::runtime_status::binary("outcome-hub-storage")?;
        self.storage
            .lock()
            .await
            .prepare(service, state, &binary)
            .await
    }
}

#[cfg(test)]
#[path = "local_storage_tests.rs"]
mod tests;
