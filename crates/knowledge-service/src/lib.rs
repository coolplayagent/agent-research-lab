//! Single-owner external knowledge gateway. Both user surfaces use this service's versioned protocol.
use anyhow::{Context, Result, ensure};
use collaboration_model::{KnowledgeWrite, KnowledgeWriteResult};
use contracts::{
    KnowledgeAccess, KnowledgeBackend, KnowledgeWriteState, OutcomeKind, OutcomeServiceKind,
};
use knowledge_model::{Configuration, Preview, Request, Target, result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use service_api::{OutcomeServiceHandshake, OutcomeServiceRequest};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

pub struct Service {
    id: String,
    state: PathBuf,
}
#[derive(Serialize, Deserialize)]
struct Receipt {
    write: KnowledgeWrite,
    target: Target,
    result: KnowledgeWriteResult,
}
impl Service {
    pub fn open(id: &str, state: &Path) -> Result<Self> {
        collaboration_model::id(id)?;
        fs::create_dir_all(state)?;
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(state, fs::Permissions::from_mode(0o700))?;
        Ok(Self {
            id: id.into(),
            state: fs::canonicalize(state)?,
        })
    }
    fn configuration(&self) -> Result<Configuration> {
        let path = self.state.join("targets.json");
        if path.exists() {
            storage::read(&path)
        } else {
            Ok(Configuration::default())
        }
    }
    fn target<'a>(&self, config: &'a Configuration, name: &str, write: bool) -> Result<&'a Target> {
        let target = config
            .targets
            .get(name)
            .context("knowledge_target_not_found")?;
        ensure!(target.enabled, "knowledge_target_disabled");
        ensure!(
            !write || target.access == KnowledgeAccess::ReadWrite,
            "knowledge_read_only"
        );
        self.location(target)?;
        Ok(target)
    }
    fn location(&self, target: &Target) -> Result<()> {
        target.validate()?;
        let path = fs::canonicalize(&target.path)?;
        ensure!(
            path == target.path
                && !path.starts_with(&self.state)
                && !self.state.starts_with(path.join(&target.content_root)),
            "knowledge_external_target_required"
        );
        Ok(())
    }
    fn inspect(
        &self,
        target: &Target,
        path: Option<&Path>,
        revision: Option<&str>,
    ) -> Result<(String, Option<String>)> {
        match target.kind {
            KnowledgeBackend::Git => knowledge_git::inspect(target, &self.state, path, revision),
            KnowledgeBackend::Directory => knowledge_directory::inspect(target, path, revision),
        }
    }
    pub fn manage(&self, request: Request) -> Result<Value> {
        let _lock = storage::lock(&self.state.join("operation.lock"))?;
        let config = self.configuration()?;
        match request {
            Request::List => Ok(json!(config)),
            Request::Save { mut configuration } => {
                ensure!(
                    configuration.revision == config.revision,
                    "knowledge_configuration_conflict"
                );
                configuration.validate()?;
                for target in configuration.targets.values() {
                    self.location(target)?;
                }
                configuration.revision = config
                    .revision
                    .checked_add(1)
                    .context("knowledge_revision_limit")?;
                storage::write(&self.state.join("targets.json"), &configuration)?;
                Ok(json!(configuration))
            }
            Request::Probe { target: name } => {
                let target = self.target(&config, &name, false)?;
                let (revision, _) = self.inspect(target, None, None)?;
                Ok(
                    json!({"target":name,"kind":target.kind,"revision":revision,"configuration_revision":config.revision,"read":true,"write":target.access==KnowledgeAccess::ReadWrite,"historical_read":target.kind==KnowledgeBackend::Git,"publication":null}),
                )
            }
            Request::Read {
                target: name,
                path,
                revision,
            } => {
                let target = self.target(&config, &name, false)?;
                let (revision, content) = self.inspect(target, Some(&path), revision.as_deref())?;
                Ok(json!({"target":name,"revision":revision,"path":path,"content":content}))
            }
            Request::Preview {
                target: name,
                outcome,
            } => {
                outcome.validate_digest()?;
                let target = self.target(&config, &name, true)?;
                let path = format!("accepted-outcomes/{}.json", outcome.input.id);
                let (revision, before) = self.inspect(target, Some(Path::new(&path)), None)?;
                let after = serde_json::to_string_pretty(&outcome)?;
                ensure!(
                    after.len() <= knowledge_model::MAX_DOCUMENT_BYTES,
                    "knowledge_document_limit"
                );
                Ok(json!(Preview {
                    target: name.clone(),
                    configuration_revision: config.revision,
                    expected_revision: format!(
                        "{revision}@{}",
                        config.publication_binding(&name, &outcome.digest)?
                    ),
                    path,
                    before,
                    content_digest: storage::digest(after.as_bytes()),
                    after
                }))
            }
        }
    }
    fn lookup(&self, id: &str, digest: &str) -> Result<Value> {
        collaboration_model::id(id)?;
        let path = self.state.join("receipts").join(format!("{id}.json"));
        if !path.exists() {
            return Ok(Value::Null);
        }
        let mut receipt: Receipt = storage::read(&path)?;
        ensure!(
            receipt.write.request_digest == digest,
            "knowledge_request_conflict"
        );
        if receipt.result.state == KnowledgeWriteState::Unknown {
            let target_state = self.target_state(&receipt.write.input.repository)?;
            let found = match receipt.target.kind {
                KnowledgeBackend::Git => serde_json::from_value(knowledge_git::dispatch_target(
                    &self.id,
                    &receipt.write.input.repository,
                    &receipt.target,
                    &target_state,
                    OutcomeServiceRequest::LookupWrite {
                        id: id.into(),
                        request_digest: digest.into(),
                    },
                )?)?,
                KnowledgeBackend::Directory => {
                    knowledge_directory::lookup(&receipt.target, &target_state, &receipt.write)?
                }
            };
            if let Some(value) = found {
                receipt.result = value;
                storage::write(&path, &receipt)?;
            }
        }
        Ok(json!(receipt.result))
    }
    fn target_state(&self, name: &str) -> Result<PathBuf> {
        collaboration_model::id(name)?;
        let path = self.state.join("targets").join(name);
        fs::create_dir_all(&path)?;
        Ok(path)
    }
    pub fn dispatch(&self, request: OutcomeServiceRequest) -> Result<Value> {
        match request {
            OutcomeServiceRequest::Probe => Ok(json!(OutcomeServiceHandshake {
                protocol_version: collaboration_model::OUTCOME_PROTOCOL_VERSION,
                id: self.id.clone(),
                kind: OutcomeServiceKind::Knowledge,
                outcome_kinds: OutcomeKind::ALL.to_vec(),
                ready: true
            })),
            OutcomeServiceRequest::Knowledge { request } => self.manage(request),
            OutcomeServiceRequest::LookupWrite { id, request_digest } => {
                let _lock = storage::lock(&self.state.join("operation.lock"))?;
                self.lookup(&id, &request_digest)
            }
            OutcomeServiceRequest::Write { write, outcome } => {
                let _lock = storage::lock(&self.state.join("operation.lock"))?;
                knowledge_model::validate_write(&self.id, &write, &outcome)?;
                ensure!(
                    serde_json::to_vec_pretty(&outcome)?.len()
                        <= knowledge_model::MAX_DOCUMENT_BYTES,
                    "knowledge_document_limit"
                );
                let old = self.lookup(&write.input.id, &write.request_digest)?;
                if !old.is_null() {
                    return Ok(old);
                }
                let config = self.configuration()?;
                let target = self.target(&config, &write.input.repository, true)?.clone();
                let (_, binding) = write
                    .input
                    .expected_revision
                    .split_once('@')
                    .context("knowledge_preview_required")?;
                ensure!(
                    binding
                        == config.publication_binding(&write.input.repository, &outcome.digest)?,
                    "knowledge_configuration_conflict"
                );
                let target_state = self.target_state(&write.input.repository)?;
                let path = self
                    .state
                    .join("receipts")
                    .join(format!("{}.json", write.input.id));
                let unknown = result(
                    &write,
                    KnowledgeWriteState::Unknown,
                    String::new(),
                    String::new(),
                    "Knowledge publication requires receipt reconciliation",
                );
                let mut receipt = Receipt {
                    write: write.clone(),
                    target: target.clone(),
                    result: unknown,
                };
                storage::write(&path, &receipt)?;
                let value = match target.kind {
                    KnowledgeBackend::Git => knowledge_git::dispatch_target(
                        &self.id,
                        &write.input.repository,
                        &target,
                        &target_state,
                        OutcomeServiceRequest::Write {
                            write: write.clone(),
                            outcome,
                        },
                    )
                    .and_then(|v| Ok(serde_json::from_value(v)?)),
                    KnowledgeBackend::Directory => {
                        knowledge_directory::publish(&target, &target_state, &write, &outcome)
                    }
                };
                if let Ok(value) = value {
                    receipt.result = value;
                }
                storage::write(&path, &receipt)?;
                Ok(json!(receipt.result))
            }
            OutcomeServiceRequest::Verify { .. } => {
                anyhow::bail!("knowledge_verification_unsupported")
            }
        }
    }
}
pub async fn serve(id: &str, state: &Path, socket: &Path, seconds: u64) -> Result<()> {
    ensure!((1..=43200).contains(&seconds), "knowledge_invalid_lifetime");
    let service = Arc::new(Service::open(id, state)?);
    let _owner = storage::lock(&state.join("owner.lock"))?;
    let listener = service_protocol::Listener::bind(socket)?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(seconds);
    let permits = Arc::new(tokio::sync::Semaphore::new(4));
    let mut tasks = tokio::task::JoinSet::new();
    loop {
        tokio::select! {
            _=tokio::time::sleep_until(deadline)=>break,
            Some(_)=tasks.join_next(),if !tasks.is_empty()=>{},
            stream=listener.accept()=>{
                let mut stream=stream?;
                let Ok(permit)=permits.clone().try_acquire_owned() else { continue; };
                let service=service.clone();
                tasks.spawn(async move {
                    let _permit=permit;
                    let result=async {
                        let envelope:service_protocol::Envelope<OutcomeServiceRequest>=tokio::time::timeout(Duration::from_secs(5),service_protocol::read(&mut stream)).await??;
                        ensure!(envelope.protocol_version==service_protocol::VERSION,"knowledge_protocol_mismatch");
                        let required = match &envelope.request { OutcomeServiceRequest::Write { write, .. } => write.input.max_seconds.saturating_add(5), _ => 60 };
                    ensure!(deadline.saturating_duration_since(tokio::time::Instant::now()).as_secs()>=required,"knowledge_service_draining");
                        tokio::task::spawn_blocking(move ||service.dispatch(envelope.request)).await?
                    }.await;
                    let _=tokio::time::timeout(Duration::from_secs(5),service_protocol::write(&mut stream,&service_protocol::Reply::from_result(result))).await;
                });
            }
        }
    }
    while tasks.join_next().await.is_some() {}
    Ok(())
}
#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
