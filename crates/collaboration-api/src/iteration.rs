//! Scenario-neutral scheduling. Evaluation and acceptance stay host-authorized.
use crate::{ScenarioAdapter, transport::*};
use anyhow::{Context, Result, ensure};
use axum::{
    extract::{Query, Request, State},
    response::Response,
};
use collaboration_model::{
    EvolutionAction, EvolutionChange, EvolutionRun, EvolutionRunInput, GoalInput,
};
use contracts::{ActivationState, IterationState, RoundState, ScenarioCapability};
use serde::Deserialize;
use serde_json::json;
use std::{
    collections::BTreeSet,
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Default, Deserialize)]
pub struct Page {
    #[serde(default)]
    group_id: String,
    #[serde(default)]
    scenario_id: String,
    #[serde(default)]
    after: String,
    #[serde(default)]
    active: bool,
}

pub async fn list(State(web): State<Web>, Query(page): Query<Page>) -> Response {
    json_response(
        web.hub
            .evolution_runs(&page.group_id, &page.scenario_id, &page.after, page.active)
            .await,
    )
}

fn adapter(scenarios: &[Arc<dyn ScenarioAdapter>], id: &str) -> Result<Arc<dyn ScenarioAdapter>> {
    let adapter = scenarios
        .iter()
        .find(|adapter| adapter.descriptor().id == id)
        .context("scenario unavailable")?
        .clone();
    let descriptor = adapter.descriptor();
    ensure!(
        descriptor.protocol_version == collaboration_model::SCENARIO_PROTOCOL_VERSION,
        "scenario protocol mismatch"
    );
    ensure!(
        descriptor
            .capabilities
            .contains(&ScenarioCapability::Execute),
        "scenario execution unsupported"
    );
    Ok(adapter)
}

async fn validate(scenarios: &[Arc<dyn ScenarioAdapter>], goal: GoalInput) -> Result<()> {
    goal.validate()?;
    let adapter = adapter(scenarios, &goal.scenario)?;
    let checked = adapter.clone();
    let input = goal.clone();
    tokio::task::spawn_blocking(move || checked.validate(&input)).await??;
    adapter.admission(&goal).await?;
    Ok(())
}

pub async fn current_goal(
    hub: &collaboration_client::Client,
    run: &EvolutionRun,
) -> Result<collaboration_model::Goal> {
    let id = &run.rounds.last().context("iteration has no round")?.goal_id;
    let mut after = String::new();
    loop {
        let page = hub
            .goals(&run.input.group_id, &run.input.scenario_id, &after, false)
            .await?;
        let goals: Vec<collaboration_model::Goal> = serde_json::from_value(page["goals"].clone())?;
        if let Some(goal) = goals.into_iter().find(|goal| goal.input.id == *id) {
            return Ok(goal);
        }
        match page["next_after"].as_str() {
            Some(next) if next != after => after = next.into(),
            _ => anyhow::bail!("iteration goal unavailable"),
        }
    }
}

fn creation_goal(input: &EvolutionRunInput) -> GoalInput {
    GoalInput {
        id: input.id.clone(),
        group_id: input.group_id.clone(),
        title: input.title.clone(),
        objective: input.objective.clone(),
        acceptance: input.acceptance.clone(),
        scenario: input.scenario_id.clone(),
        parameters: input.parameters.clone(),
        assignments: input.assignments.clone(),
        max_seconds: input.max_seconds,
    }
}

pub async fn manage(State(web): State<Web>, request: Request) -> Response {
    json_response(
        async {
            intent(request.headers(), web.addr, "manage-crystal")?;
            let _permit = web
                .reads
                .clone()
                .try_acquire_owned()
                .context("iteration management backpressure")?;
            let change: EvolutionChange =
                serde_json::from_slice(&body(request, 192 * 1024).await?)?;
            let scenarios = crate::scenarios::configured(&web.scenarios, web.services.as_deref())?;
            if !matches!(
                &change.action,
                EvolutionAction::Create { .. }
                    | EvolutionAction::StartRound { .. }
                    | EvolutionAction::Pause
                    | EvolutionAction::Resume
                    | EvolutionAction::Stop
            ) {
                let run = web.hub.evolution_run(&change.run_id).await?;
                ensure!(
                    adapter(&scenarios, &run.input.scenario_id)?
                        .descriptor()
                        .manual_review,
                    "scenario requires its independent host evidence gate"
                );
            }
            match &change.action {
                EvolutionAction::Create { input } => {
                    validate(&scenarios, creation_goal(input)).await?
                }
                EvolutionAction::StartRound { goal } => validate(&scenarios, goal.clone()).await?,
                EvolutionAction::BeginActivation { .. }
                | EvolutionAction::Activate { .. }
                | EvolutionAction::FinishRound {
                    state: RoundState::Completed,
                    ..
                } => {
                    let run = web.hub.evolution_run(&change.run_id).await?;
                    let descriptor = adapter(&scenarios, &run.input.scenario_id)?.descriptor();
                    let activation = descriptor
                        .capabilities
                        .contains(&ScenarioCapability::Activate);
                    if matches!(
                        change.action,
                        EvolutionAction::BeginActivation { .. } | EvolutionAction::Activate { .. }
                    ) {
                        ensure!(activation, "scenario activation unsupported");
                    } else if activation {
                        ensure!(
                            run.rounds
                                .last()
                                .and_then(|round| round.activation.as_ref())
                                .is_some_and(|record| record.state == ActivationState::Active),
                            "scenario requires verified activation before completing the round"
                        );
                    }
                }
                _ => {}
            }
            Ok(json!(web.hub.evolution_change(change).await?))
        }
        .await,
    )
}

/// A paused run leaves already claimed work bounded, while blocking new claims.
pub async fn suspended_goals(hub: &collaboration_client::Client) -> Result<BTreeSet<String>> {
    let mut goals = BTreeSet::new();
    let mut after = String::new();
    loop {
        let page = hub.evolution_runs("", "", &after, true).await?;
        let runs: Vec<EvolutionRun> = serde_json::from_value(page["runs"].clone())?;
        for run in runs {
            if run.state == IterationState::Paused {
                goals.extend(run.rounds.last().map(|round| round.goal_id.clone()));
            }
        }
        match page["next_after"].as_str() {
            Some(next) if next != after => after = next.into(),
            Some(_) => anyhow::bail!("iteration pagination did not advance"),
            None => return Ok(goals),
        }
    }
}

/// One revision-fenced reconciliation is safe to repeat after process recovery.
pub async fn advance(
    hub: &collaboration_client::Client,
    scenarios: &[Arc<dyn ScenarioAdapter>],
    run: EvolutionRun,
) -> Result<()> {
    if run.state != IterationState::Running {
        return Ok(());
    }
    let scenario = adapter(scenarios, &run.input.scenario_id)?;
    if run
        .rounds
        .last()
        .is_none_or(|round| round.finished_ms.is_some())
    {
        let goal = run.next_goal()?;
        validate(scenarios, goal.clone()).await?;
        hub.evolution_change(EvolutionChange {
            request_id: format!(
                "round-{}-{}",
                &storage::digest(run.input.id.as_bytes())[..32],
                run.rounds.len() + 1
            ),
            run_id: run.input.id,
            revision: run.revision,
            action: EvolutionAction::StartRound { goal },
        })
        .await?;
    } else {
        scenario.reconcile(hub, &run).await?;
    }
    Ok(())
}

pub async fn run(
    hub: collaboration_client::Client,
    scenarios: Arc<Vec<Arc<dyn ScenarioAdapter>>>,
    seconds: u64,
    stop: tokio::sync::watch::Receiver<bool>,
) {
    run_with_registry(hub, scenarios, None, seconds, stop).await;
}

pub async fn run_with_registry(
    hub: collaboration_client::Client,
    scenarios: Arc<Vec<Arc<dyn ScenarioAdapter>>>,
    registry: Option<Arc<service_registry::Registry>>,
    seconds: u64,
    mut stop: tokio::sync::watch::Receiver<bool>,
) {
    let started = Instant::now();
    let mut interval = tokio::time::interval(Duration::from_secs(2));
    let mut tasks = tokio::task::JoinSet::new();
    let mut running = BTreeSet::new();
    let mut after = String::new();
    loop {
        if *stop.borrow() || started.elapsed().as_secs() >= seconds.saturating_sub(15) {
            break;
        }
        tokio::select! {
            _ = stop.changed() => break,
            Some(result) = tasks.join_next(), if !tasks.is_empty() => {
                if let Ok(id) = result { running.remove(&id); }
            }
            _ = interval.tick() => {
                let Ok(scenarios) = crate::scenarios::configured(&scenarios, registry.as_deref()) else { continue };
                'pages: while let Ok(page) = hub.evolution_runs("", "", &after, true).await {
                    let Ok(runs) = serde_json::from_value::<Vec<EvolutionRun>>(page["runs"].clone()) else { break };
                    for run in runs {
                        if tasks.len() >= 4 { break 'pages; }
                        after = run.input.id.clone();
                        if run.state != IterationState::Running || !running.insert(run.input.id.clone()) { continue; }
                        let hub = hub.clone();
                        let scenarios = scenarios.clone();
                        tasks.spawn(async move {
                            let id = run.input.id.clone();
                            let _ = tokio::time::timeout(Duration::from_secs(10), advance(&hub, &scenarios, run)).await;
                            id
                        });
                    }
                    match page["next_after"].as_str() {
                        Some(next) => after = next.into(),
                        None => { after.clear(); break; },
                    }
                }
            }
        }
    }
    tasks.shutdown().await;
}
