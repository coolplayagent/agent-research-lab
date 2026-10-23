//! A separate system role reuses an explicitly pinned execution configuration.
use crate::*;
impl Registry {
    pub fn record_task_diagnostic(
        &self,
        task: &str,
        claim: &str,
        error: &anyhow::Error,
        receipt: Option<&ExecutionReceipt>,
    ) {
        let name = format!(
            "task-{}.diagnostic.json",
            &storage::digest(task.as_bytes())[..32]
        );
        let details: String = format!("{error:#}").chars().take(8192).collect();
        let receipt =
            receipt.filter(|r| serde_json::to_vec(r).is_ok_and(|bytes| bytes.len() <= 65536));
        let _ = storage::write(
            &self.path.with_file_name(name),
            &serde_json::json!({"task_id":task,"claim_id":claim,"details":details,"execution":service_protocol::private_details(error),"receipt":receipt}),
        );
    }

    pub async fn coordinator_probe_lock(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.coordinator_probe.lock().await
    }
    pub fn coordinator_configuration(&self) -> Result<Configuration> {
        let mut config = self.configuration();
        if config.coordinator.is_none() {
            let mut verified: Vec<_> = config
                .bindings
                .iter()
                .filter_map(|(person, id)| {
                    let a = config.adapters.iter().find(|a| a.id == *id && a.enabled)?;
                    let e = config.binding_evidence.get(person)?;
                    (storage::digest(&serde_json::to_vec(a).ok()?) == e.adapter_digest)
                        .then_some((e.verified_unix_ms, a))
                })
                .collect();
            verified.sort_by_key(|(when, a)| (*when, a.id.clone()));
            let adapter = verified
                .first()
                .map(|(_, a)| *a)
                .context("task_executor_unavailable")?;
            config.coordinator = Some(CoordinatorConfiguration {
                adapter_id: adapter.id.clone(),
                model: None,
                adapter_digest: storage::digest(&serde_json::to_vec(adapter)?),
                protocol_evidence: None,
            });
            config = self.save(config)?;
        }
        // Repair older persisted selections, then require a fresh protocol probe.
        if Self::refresh_coordinator(&mut config)? {
            config = self.save(config)?;
        }
        Self::coordinator_adapter(&config)?;
        Ok(config)
    }
    pub(super) fn refresh_coordinator(config: &mut Configuration) -> Result<bool> {
        let Some(selected) = &mut config.coordinator else {
            return Ok(false);
        };
        let mut changed = false;
        if let Some(adapter) = config.adapters.iter().find(|a| a.id == selected.adapter_id) {
            let digest = storage::digest(&serde_json::to_vec(adapter)?);
            if selected.adapter_digest != digest {
                selected.adapter_digest = digest;
                selected.protocol_evidence = None;
                changed = true;
            }
        }
        if config
            .coordinator
            .as_ref()
            .unwrap()
            .protocol_evidence
            .is_some()
            && !Self::coordinator_probe_valid(config)
        {
            config.coordinator.as_mut().unwrap().protocol_evidence = None;
            changed = true;
        }
        Ok(changed)
    }
    pub fn coordinator_adapter(config: &Configuration) -> Result<Adapter> {
        let selected = config
            .coordinator
            .as_ref()
            .context("Coordinator has no selected adapter")
            .context(contracts::TaskErrorCode::Unavailable)?;
        let mut adapter = config
            .adapters
            .iter()
            .find(|a| a.id == selected.adapter_id && a.enabled)
            .context("Selected coordinator adapter is missing or disabled")
            .context(contracts::TaskErrorCode::Unavailable)?
            .clone();
        ensure!(
            storage::digest(&serde_json::to_vec(&adapter)?) == selected.adapter_digest,
            "task_executor_unavailable: Coordinator adapter revision requires protocol validation"
        );
        if let Some(model) = &selected.model {
            ensure!(
                !model.trim().is_empty() && model.len() <= 128 && !model.contains('\0'),
                "task_invalid"
            );
            ensure!(
                std::path::Path::new(&adapter.command[0])
                    .file_stem()
                    .is_some_and(|s| s == "outcome-hub-agent"),
                "task_invalid"
            );
            if let Some(index) = adapter.command.iter().position(|s| s == "--model") {
                *adapter.command.get_mut(index + 1).context("task_invalid")? = model.clone();
            } else {
                adapter.command.extend(["--model".into(), model.clone()]);
            }
        }
        Ok(adapter)
    }
    pub fn coordinator_fingerprint(config: &Configuration) -> Result<String> {
        let adapter = Self::coordinator_adapter(config)?;
        let executor = config.service(&adapter.executor_id, contracts::ServiceKind::Executor)?;
        let sandbox = config.service(
            executor
                .sandbox_id
                .as_deref()
                .context("task_executor_unavailable")?,
            contracts::ServiceKind::Sandbox,
        )?;
        Ok(storage::digest(&serde_json::to_vec(&(
            "coordinator-probe.v2-message-contract",
            adapter,
            executor,
            sandbox,
        ))?))
    }
    pub fn coordinator_probe_valid(config: &Configuration) -> bool {
        Self::coordinator_fingerprint(config)
            .ok()
            .is_some_and(|digest| {
                config
                    .coordinator
                    .as_ref()
                    .and_then(|c| c.protocol_evidence.as_ref())
                    .is_some_and(|e| e.starts_with(&(digest + ":")))
            })
    }
    pub fn record_coordinator_probe(&self, expected: &Configuration, digest: String) -> Result<()> {
        let mut config = self.configuration();
        ensure!(
            serde_json::to_value(&config.coordinator)?
                == serde_json::to_value(&expected.coordinator)?,
            "task_conflict"
        );
        let fingerprint = Self::coordinator_fingerprint(expected)?;
        ensure!(
            Self::coordinator_fingerprint(&config)? == fingerprint,
            "task_conflict"
        );
        config
            .coordinator
            .as_mut()
            .context("task_executor_unavailable")?
            .protocol_evidence = Some(format!("{fingerprint}:{digest}"));
        self.save(config)?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "coordinator_tests.rs"]
mod tests;
