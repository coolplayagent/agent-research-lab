//! Explicit, receipt-backed publication to an optional external knowledge repository.
use crate::{outcomes::*, transport::*, *};
use axum::extract::{Request, State};
use axum::response::Response;
use collaboration_model::{
    KnowledgeWrite, KnowledgeWriteInput, KnowledgeWriteResult, OutcomeCommand, OutcomeView,
};
use contracts::{KnowledgeWriteState, OutcomeErrorCode, OutcomeServiceKind};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WriteInput {
    service_id: Option<String>,
    repository: Option<String>,
    request_id: String,
    outcome_id: String,
    expected_revision: String,
    configuration_revision: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReconcileInput {
    outcome_id: String,
    request_id: String,
}

pub async fn write(State(web): State<Web>, request: Request) -> Response {
    outcome_response(async {
        intent(request.headers(),web.addr,"manage-outcomes")?;
        let input: WriteInput = serde_json::from_slice(&body(request,4096).await?)?;
        let config = web.services.as_ref().context(OutcomeErrorCode::NotFound)?.configuration();
        ensure!(config.revision == input.configuration_revision,"{}",OutcomeErrorCode::BindingChanged);
        let entry: OutcomeView = serde_json::from_value(web.hub.outcomes(None,OutcomeCommand::Get {id:input.outcome_id}).await?)?;
        let selected;
        let target = match (input.service_id,input.repository) {
            (Some(service_id),Some(repository)) => { selected=service_api::KnowledgeTarget {service_id,repository}; &selected },
            (None,None) => config.knowledge_goals.get(&entry.outcome.input.goal_id).or_else(|| config.knowledge_groups.get(&entry.outcome.input.group_id)).context(OutcomeErrorCode::NotFound)?,
            _ => anyhow::bail!("{}",OutcomeErrorCode::Invalid),
        };
        let service = configured_service(&config,&target.service_id,OutcomeServiceKind::Knowledge)?;
        ensure!(service.enabled && service.outcome_kinds.contains(&entry.outcome.input.kind),"{}",OutcomeErrorCode::Invalid);
        web.services.as_ref().context(OutcomeErrorCode::NotFound)?.prepare_knowledge(&service).await?;
        let permit = web.outcome_tasks.clone().try_acquire_owned().context(OutcomeErrorCode::Capacity)?;
        let response = web.hub.outcomes(None,OutcomeCommand::BeginWrite {input:KnowledgeWriteInput {
            id:input.request_id,outcome_id:entry.outcome.input.id.clone(),outcome_digest:entry.outcome.digest.clone(),
            acceptance_id:entry.latest_decision.as_ref().context(OutcomeErrorCode::VerificationRequired)?.id.clone(),
            service_id:service.id.clone(),service_digest:service.digest()?,repository:target.repository.clone(),expected_revision:input.expected_revision,max_seconds:service.max_seconds,
        }}).await?;
        if response["created"] != true { return Ok(response); }
        let write: KnowledgeWrite = serde_json::from_value(response["record"].clone())?;
        tokio::spawn(async move {
            let _permit = permit;
            let mut shutdown = web.shutdown.subscribe();
            let result = if service_ready(&web,&service).await.is_err() || write.deadline_ms <= collaboration_model::now_ms() || !currently_accepted(&web, &write).await {
                terminal(&write,KnowledgeWriteState::Failed,"Knowledge write was not dispatched")
            } else {
                let result = tokio::select! {
                    response = service_api::write_outcome(&service,write.clone(),entry.outcome) => response,
                    _ = shutdown.changed() => Err(anyhow::anyhow!("service stopped")),
                };
                match result {
                    Ok(result) if result.id == write.input.id && result.request_digest == write.request_digest => result,
                    _ => terminal(&write,KnowledgeWriteState::Unknown,"Knowledge write requires receipt reconciliation"),
                }
            };
            if web.hub.outcomes(None,OutcomeCommand::FinishWrite {result}).await.is_err() {
                let _ = web.hub.outcomes(None,OutcomeCommand::FinishWrite {result:terminal(&write,KnowledgeWriteState::Unknown,"Knowledge receipt could not be admitted")}).await;
            }
        });
        Ok(response)
    }.await)
}

async fn currently_accepted(web: &Web, write: &KnowledgeWrite) -> bool {
    let Ok(value) = web
        .hub
        .outcomes(
            None,
            OutcomeCommand::Get {
                id: write.input.outcome_id.clone(),
            },
        )
        .await
    else {
        return false;
    };
    let Ok(entry) = serde_json::from_value::<OutcomeView>(value) else {
        return false;
    };
    entry.acceptance == contracts::OutcomeAcceptanceState::Accepted
        && entry.outcome.digest == write.input.outcome_digest
        && entry
            .latest_decision
            .is_some_and(|decision| decision.id == write.input.acceptance_id)
}

fn terminal(
    write: &KnowledgeWrite,
    state: KnowledgeWriteState,
    summary: &str,
) -> KnowledgeWriteResult {
    KnowledgeWriteResult {
        id: write.input.id.clone(),
        request_digest: write.request_digest.clone(),
        state,
        reference: String::new(),
        revision: String::new(),
        summary: summary.into(),
    }
}

pub async fn reconcile(State(web): State<Web>, request: Request) -> Response {
    outcome_response(
        async {
            intent(request.headers(), web.addr, "manage-outcomes")?;
            let input: ReconcileInput = serde_json::from_slice(&body(request, 4096).await?)?;
            let entry: OutcomeView = serde_json::from_value(
                web.hub
                    .outcomes(
                        None,
                        OutcomeCommand::Get {
                            id: input.outcome_id,
                        },
                    )
                    .await?,
            )?;
            let write = entry.latest_write.context(OutcomeErrorCode::NotFound)?;
            ensure!(
                write.input.id == input.request_id,
                "{}",
                OutcomeErrorCode::Conflict
            );
            if write.result.as_ref().is_some_and(|result| {
                matches!(
                    result.state,
                    KnowledgeWriteState::Applied | KnowledgeWriteState::Failed
                )
            }) {
                return Ok(json!(write));
            }
            let config = web
                .services
                .as_ref()
                .context(OutcomeErrorCode::NotFound)?
                .configuration();
            let service = configured_service(
                &config,
                &write.input.service_id,
                OutcomeServiceKind::Knowledge,
            )?;
            ensure!(
                service.digest()? == write.input.service_digest,
                "{}",
                OutcomeErrorCode::BindingChanged
            );
            service_ready(&web, &service).await?;
            let result: Option<KnowledgeWriteResult> = service_protocol::call(
                &service.endpoint,
                &service_api::OutcomeServiceRequest::LookupWrite {
                    id: write.input.id.clone(),
                    request_digest: write.request_digest.clone(),
                },
                Duration::from_secs(8),
            )
            .await?;
            let result = result.unwrap_or_else(|| {
                terminal(
                    &write,
                    KnowledgeWriteState::Unknown,
                    "Knowledge service has no conclusive receipt",
                )
            });
            ensure!(
                result.id == write.input.id && result.request_digest == write.request_digest,
                "{}",
                OutcomeErrorCode::BindingChanged
            );
            web.hub
                .outcomes(None, OutcomeCommand::FinishWrite { result })
                .await
        }
        .await,
    )
}
