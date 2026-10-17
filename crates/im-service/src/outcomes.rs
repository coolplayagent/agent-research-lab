//! Outcome HTTP boundary: Agents publish proposals; only host services verify and accept.
use crate::transport::*;
use crate::*;
use axum::extract::{Query, Request, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use contracts::{OutcomeErrorCode, OutcomeServiceKind, VerificationRunState};
use crystal::{
    OutcomeCommand, OutcomeQuery, OutcomeVerification, OutcomeVerificationInput,
    OutcomeVerificationResult, OutcomeView,
};
use serde::Deserialize;

pub(super) fn outcome_response(result: Result<Value>) -> Response {
    match result {
        Ok(value) => axum::Json(value).into_response(),
        Err(error) => {
            let message = error.to_string();
            let code = OutcomeErrorCode::ALL
                .iter()
                .copied()
                .find(|code| message.contains(code.as_str()))
                .unwrap_or(OutcomeErrorCode::Invalid);
            let status = match code {
                OutcomeErrorCode::Forbidden => StatusCode::FORBIDDEN,
                OutcomeErrorCode::NotFound => StatusCode::NOT_FOUND,
                OutcomeErrorCode::Conflict
                | OutcomeErrorCode::StaleAttempt
                | OutcomeErrorCode::BindingChanged => StatusCode::CONFLICT,
                OutcomeErrorCode::Capacity => StatusCode::TOO_MANY_REQUESTS,
                _ => StatusCode::BAD_REQUEST,
            };
            (
                status,
                axum::Json(json!({"code":code,"error":{"code":code,"parameters":{}}})),
            )
                .into_response()
        }
    }
}

#[derive(Default, Deserialize)]
pub struct Page {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub topic_id: String,
    #[serde(default)]
    pub goal_id: String,
    #[serde(default)]
    pub query: String,
    #[serde(default)]
    pub after: u64,
    #[serde(default)]
    pub snapshot: Option<u64>,
    #[serde(default)]
    pub kind: Option<contracts::OutcomeKind>,
}
pub async fn list(State(web): State<Web>, Query(page): Query<Page>) -> Response {
    let command = if page.id.is_empty() {
        OutcomeCommand::Search {
            query: OutcomeQuery {
                topic_id: page.topic_id,
                goal_id: page.goal_id,
                query: page.query,
                after: page.after,
                snapshot: page.snapshot,
                kind: page.kind,
            },
        }
    } else {
        OutcomeCommand::Get { id: page.id }
    };
    outcome_response(web.hub.outcomes(None, command).await)
}

pub async fn agent(State(web): State<Web>, request: Request) -> Response {
    outcome_response(
        async {
            let credential = token(request.headers())?.to_owned();
            let command: OutcomeCommand = serde_json::from_slice(&body(request, 64 * 1024).await?)?;
            ensure!(command.agent_allowed(), "{}", OutcomeErrorCode::Forbidden);
            validate_source_binding(&web, &command)?;
            let publish = matches!(command, OutcomeCommand::Publish { .. });
            let result = web.hub.outcomes(Some(&credential), command).await?;
            if publish {
                auto_verify(&web, &result).await;
            }
            Ok(result)
        }
        .await,
    )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Verify {
    request_id: String,
    outcome_id: String,
    verifier_id: String,
    configuration_revision: u64,
}

pub async fn manage(State(web): State<Web>, request: Request) -> Response {
    outcome_response(
        async {
            intent(request.headers(), web.addr, "manage-outcomes")?;
            let command: OutcomeCommand = serde_json::from_slice(&body(request, 64 * 1024).await?)?;
            validate_source_binding(&web, &command)?;
            ensure!(
                !matches!(
                    command,
                    OutcomeCommand::BeginVerification { .. }
                        | OutcomeCommand::FinishVerification { .. }
                        | OutcomeCommand::BeginWrite { .. }
                        | OutcomeCommand::FinishWrite { .. }
                ),
                "{}",
                OutcomeErrorCode::Forbidden
            );
            let publish = matches!(command, OutcomeCommand::Publish { .. });
            let result = web.hub.outcomes(None, command).await?;
            if publish {
                auto_verify(&web, &result).await;
            }
            Ok(result)
        }
        .await,
    )
}

pub async fn verify(State(web): State<Web>, request: Request) -> Response {
    outcome_response(
        async {
            intent(request.headers(), web.addr, "manage-outcomes")?;
            let input: Verify = serde_json::from_slice(&body(request, 4096).await?)?;
            let registry = web.services.as_ref().context(OutcomeErrorCode::NotFound)?;
            let config = registry.configuration();
            ensure!(
                config.revision == input.configuration_revision,
                "{}",
                OutcomeErrorCode::BindingChanged
            );
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
            start_verify(&web, &config, &entry, input.request_id, &input.verifier_id).await
        }
        .await,
    )
}

async fn auto_verify(web: &Web, result: &Value) {
    let Some(registry) = &web.services else {
        return;
    };
    let Ok(entry) = serde_json::from_value::<OutcomeView>(result.clone()) else {
        return;
    };
    let config = registry.configuration();
    let Some(id) = config
        .outcome_bindings
        .get(&entry.outcome.input.topic_id)
        .and_then(|binding| binding.verifiers.get(&entry.outcome.input.kind))
    else {
        return;
    };
    let request_id = format!(
        "auto-{}",
        storage::digest(format!("{}:{id}", entry.outcome.digest).as_bytes())
    );
    if let Err(error) = start_verify(web, &config, &entry, request_id, id).await {
        eprintln!("Outcome automatic verification was not started: {error}");
    }
}

pub(super) fn configured_service(
    config: &service_api::Configuration,
    id: &str,
    kind: OutcomeServiceKind,
) -> Result<service_api::OutcomeService> {
    config
        .outcome_services
        .iter()
        .find(|service| service.id == id && service.kind == kind)
        .cloned()
        .context(OutcomeErrorCode::NotFound)
}

async fn start_verify(
    web: &Web,
    config: &service_api::Configuration,
    entry: &OutcomeView,
    request_id: String,
    id: &str,
) -> Result<Value> {
    let service = configured_service(config, id, OutcomeServiceKind::Verifier)?;
    ensure!(
        service.outcome_kinds.contains(&entry.outcome.input.kind),
        "{}",
        OutcomeErrorCode::Invalid
    );
    let permit = web
        .outcome_tasks
        .clone()
        .try_acquire_owned()
        .context(OutcomeErrorCode::Capacity)?;
    let result = web
        .hub
        .outcomes(
            None,
            OutcomeCommand::BeginVerification {
                input: OutcomeVerificationInput {
                    id: request_id,
                    outcome_id: entry.outcome.input.id.clone(),
                    outcome_digest: entry.outcome.digest.clone(),
                    verifier_id: service.id.clone(),
                    verifier_digest: service.digest()?,
                    max_seconds: service.max_seconds,
                },
            },
        )
        .await?;
    if result["created"] != true {
        return Ok(result);
    }
    let run: OutcomeVerification = serde_json::from_value(result["record"].clone())?;
    let web = web.clone();
    let outcome = entry.outcome.clone();
    tokio::spawn(async move {
        let _permit = permit;
        let mut shutdown = web.shutdown.subscribe();
        let checked = service_ready(&web, &service).await;
        let output = match checked {
            Err(_) => OutcomeVerificationResult {
                id: run.input.id.clone(),
                request_digest: run.request_digest.clone(),
                state: VerificationRunState::Unavailable,
                summary: "Verifier service is unavailable".into(),
                evidence: vec![],
            },
            Ok(()) => {
                let response = tokio::select! {
                    response = service_api::verify_outcome(&service,run.clone(),outcome) => response,
                    _ = shutdown.changed() => Err(anyhow::anyhow!("service stopped")),
                };
                match response {
                    Ok(result)
                        if result.id == run.input.id
                            && result.request_digest == run.request_digest
                            && service_configuration_current(&web, &service).is_ok() =>
                    {
                        result
                    }
                    _ => OutcomeVerificationResult {
                        id: run.input.id.clone(),
                        request_digest: run.request_digest.clone(),
                        state: VerificationRunState::Unknown,
                        summary: "Verification did not produce a matching terminal receipt".into(),
                        evidence: vec![],
                    },
                }
            }
        };
        if web
            .hub
            .outcomes(None, OutcomeCommand::FinishVerification { result: output })
            .await
            .is_err()
        {
            let _ = web
                .hub
                .outcomes(
                    None,
                    OutcomeCommand::FinishVerification {
                        result: OutcomeVerificationResult {
                            id: run.input.id,
                            request_digest: run.request_digest,
                            state: VerificationRunState::Unknown,
                            summary: "Verification receipt could not be admitted".into(),
                            evidence: vec![],
                        },
                    },
                )
                .await;
        }
    });
    Ok(result)
}

fn validate_source_binding(web: &Web, command: &OutcomeCommand) -> Result<()> {
    if let OutcomeCommand::Publish { input } = command
        && let Some(binding) = &input.knowledge
    {
        let config = web
            .services
            .as_ref()
            .context(OutcomeErrorCode::NotFound)?
            .configuration();
        let target = config
            .outcome_bindings
            .get(&input.topic_id)
            .and_then(|value| value.knowledge.as_ref())
            .context(OutcomeErrorCode::BindingChanged)?;
        ensure!(
            target.service_id == binding.service_id && target.repository == binding.repository,
            "{}",
            OutcomeErrorCode::BindingChanged
        );
    }
    Ok(())
}
pub(super) fn service_configuration_current(
    web: &Web,
    service: &service_api::OutcomeService,
) -> Result<service_api::Configuration> {
    let config = web
        .services
        .as_ref()
        .context("service registry unavailable")?
        .configuration();
    let current = configured_service(&config, &service.id, service.kind)?;
    ensure!(
        current.enabled && current.digest()? == service.digest()?,
        "outcome service configuration changed"
    );
    Ok(config)
}
pub(super) async fn service_ready(web: &Web, service: &service_api::OutcomeService) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(8), async {
        let config = service_configuration_current(web, service)?;
        for id in &service.dependencies {
            let dependency = config
                .services
                .iter()
                .find(|entry| entry.id == *id && entry.enabled)
                .context("outcome service dependency unavailable")?;
            service_api::probe(&dependency.endpoint, dependency.kind).await?;
        }
        service_api::probe_outcome_service(service).await?;
        Ok(())
    })
    .await?
}
