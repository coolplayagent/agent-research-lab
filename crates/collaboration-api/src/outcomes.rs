//! Outcome HTTP boundary: Agents publish proposals; only host services verify and accept.
use crate::transport::*;
use crate::*;
use axum::extract::{Query, Request, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use collaboration_model::{
    OutcomeCommand, OutcomeQuery, OutcomeVerification, OutcomeVerificationInput,
    OutcomeVerificationResult, OutcomeView,
};
use contracts::{OutcomeErrorCode, OutcomeServiceKind, VerificationRunState};
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
    pub group_id: String,
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
                group_id: page.group_id,
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
            validate_source_binding(&web, &command).await?;
            web.hub.outcomes(Some(&credential), command).await
        }
        .await,
    )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HostVerify {
    request_id: String,
    outcome_id: String,
    state: VerificationRunState,
    summary: String,
    evidence_indices: Vec<usize>,
}

pub async fn manage(State(web): State<Web>, request: Request) -> Response {
    outcome_response(
        async {
            intent(request.headers(), web.addr, "manage-outcomes")?;
            let command: OutcomeCommand = serde_json::from_slice(&body(request, 64 * 1024).await?)?;
            validate_source_binding(&web, &command).await?;
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
            web.hub.outcomes(None, command).await
        }
        .await,
    )
}

pub async fn host_verify(State(web): State<Web>, request: Request) -> Response {
    outcome_response(
        async {
            intent(request.headers(), web.addr, "manage-outcomes")?;
            let input: HostVerify = serde_json::from_slice(&body(request, 8192).await?)?;
            ensure!(
                matches!(
                    input.state,
                    VerificationRunState::Passed | VerificationRunState::Failed
                ),
                "{}",
                OutcomeErrorCode::Invalid
            );
            ensure!(
                !input.summary.trim().is_empty() && input.summary.len() <= 4096,
                "{}",
                OutcomeErrorCode::Invalid
            );
            ensure!(
                !input.evidence_indices.is_empty() && input.evidence_indices.len() <= 16,
                "{}",
                OutcomeErrorCode::Invalid
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
            let mut selected = std::collections::BTreeSet::new();
            let mut evidence = Vec::with_capacity(input.evidence_indices.len());
            for index in input.evidence_indices {
                ensure!(selected.insert(index), "{}", OutcomeErrorCode::Invalid);
                evidence.push(
                    entry
                        .outcome
                        .input
                        .evidence
                        .get(index)
                        .cloned()
                        .context(OutcomeErrorCode::Invalid)?,
                );
            }
            let verifier_digest = collaboration_model::digest(b"host-review-v1");
            let started = web
                .hub
                .outcomes(
                    None,
                    OutcomeCommand::BeginVerification {
                        input: OutcomeVerificationInput {
                            id: input.request_id.clone(),
                            outcome_id: entry.outcome.input.id.clone(),
                            outcome_digest: entry.outcome.digest.clone(),
                            verifier_id: "host-review".into(),
                            verifier_digest,
                            max_seconds: 600,
                        },
                    },
                )
                .await?;
            let run: OutcomeVerification = serde_json::from_value(started["record"].clone())?;
            web.hub
                .outcomes(
                    None,
                    OutcomeCommand::FinishVerification {
                        result: OutcomeVerificationResult {
                            id: run.input.id,
                            request_digest: run.request_digest,
                            state: input.state,
                            summary: input.summary,
                            evidence,
                        },
                    },
                )
                .await
        }
        .await,
    )
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

async fn validate_source_binding(web: &Web, command: &OutcomeCommand) -> Result<()> {
    if let OutcomeCommand::Publish { input } = command
        && let Some(binding) = &input.knowledge
    {
        let config = web
            .services
            .as_ref()
            .context(OutcomeErrorCode::NotFound)?
            .configuration();
        let target = config
            .knowledge_goals
            .get(&input.goal_id)
            .or_else(|| config.knowledge_groups.get(&input.group_id));
        ensure!(
            target.is_some_and(|target| target.service_id == binding.service_id
                && target.repository == binding.repository),
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

pub async fn retired() -> Response {
    (StatusCode::GONE,axum::Json(json!({"error":{"code":OutcomeErrorCode::ProtocolUpgradeRequired,"parameters":{}},"code":OutcomeErrorCode::ProtocolUpgradeRequired}))).into_response()
}
