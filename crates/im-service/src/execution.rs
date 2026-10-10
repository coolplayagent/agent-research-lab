//! Bound execution to the current goal claim. Agent reports never accept a goal.
use anyhow::{Context, Result, ensure};
use contracts::{ServiceKind, WorkState};
use crystal::{Goal, WorkChange, WorkItem, WorkResult};
use serde_json::json;
use service_api::{AgentInput, Configuration, ExecutionReceipt, ExecutorRequest, Registry};
use std::{
    io::Read,
    sync::Arc,
    time::{Duration, Instant},
};
async fn advance(
    hub: im_storage::Client,
    config: Configuration,
    goal: Goal,
    work: WorkItem,
    remaining: u64,
) -> Result<()> {
    let id = config
        .bindings
        .get(&work.person_id)
        .context("Agent has no executor binding")?;
    let adapter = config
        .adapters
        .iter()
        .find(|a| a.id == *id && a.enabled)
        .context("adapter is disabled")?
        .clone();
    let executor = config.service(&adapter.executor_id, ServiceKind::Executor)?;
    let sandbox = config.service(
        executor
            .sandbox_id
            .as_deref()
            .context("sandbox dependency missing")?,
        ServiceKind::Sandbox,
    )?;
    let seconds = goal
        .input
        .max_seconds
        .saturating_sub(20)
        .min(adapter.max_seconds)
        .min(remaining.saturating_sub(20));
    ensure!(seconds > 0, "service execution window exhausted");
    // Probe before claiming. An unavailable dependency leaves work ready for recovery.
    service_api::probe(&executor.endpoint, ServiceKind::Executor).await?;
    service_api::probe(&sandbox.endpoint, ServiceKind::Sandbox).await?;
    let mut random = [0u8; 32];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut random)?;
    let claim_id = storage::digest(&random);
    hub.work_as(
        &work.person_id,
        WorkChange::Claim {
            attempt: work.attempt,
            goal_id: goal.input.id.clone(),
            work_id: work.id.clone(),
            claim_id: claim_id.clone(),
        },
    )
    .await?;
    let result=async {
  let messages=hub.history_before(&goal.input.group_id,i64::MAX as u64,32).await?;
  let input=AgentInput{protocol_version:service_protocol::VERSION,request_id:claim_id.clone(),person_id:work.person_id.clone(),goal_id:goal.input.id.clone(),work_id:work.id.clone(),attempt:work.attempt,objective:goal.input.objective.clone(),acceptance:goal.input.acceptance.clone(),instruction:work.instruction.clone(),messages};
  let receipt:ExecutionReceipt=service_protocol::call(&executor.endpoint,&ExecutorRequest::Execute{adapter:adapter.clone(),sandbox:sandbox.endpoint.clone(),input:Box::new(input.clone()),seconds},Duration::from_secs(seconds+15)).await?;
  receipt.output.validate(&input)?;
  ensure!(receipt.adapter_id==adapter.id && receipt.agent==adapter.agent && receipt.input_sha256==storage::digest(&serde_json::to_vec(&input)?) && receipt.adapter_sha256==storage::digest(&serde_json::to_vec(&adapter)?) && receipt.output_sha256==storage::digest(&serde_json::to_vec(&receipt.output)?),"executor receipt binding mismatch");
  Ok::<_,anyhow::Error>(WorkResult{code:None,summary:receipt.output.summary,succeeded:receipt.output.succeeded,evidence:json!({"adapter_id":receipt.adapter_id,"agent":receipt.agent,"protocol_version":service_protocol::VERSION,"input_sha256":receipt.input_sha256,"adapter_sha256":receipt.adapter_sha256,"output_sha256":receipt.output_sha256,"sandbox_id":receipt.sandbox_id,"elapsed_ms":receipt.elapsed_ms,"agent_evidence":receipt.output.evidence})})
 }.await.unwrap_or_else(|error|WorkResult{code:Some(contracts::WorkResultCode::ExecutorFailed),summary:"executor_failed".into(),succeeded:false,evidence:json!({"error":error.to_string().chars().take(1000).collect::<String>()})});
    hub.work_as(
        &work.person_id,
        WorkChange::Submit {
            attempt: work.attempt,
            goal_id: goal.input.id,
            work_id: work.id,
            claim_id,
            result,
        },
    )
    .await?;
    Ok(())
}
pub async fn run(
    hub: im_storage::Client,
    registry: Arc<Registry>,
    seconds: u64,
    mut stop: tokio::sync::watch::Receiver<bool>,
) {
    let started = Instant::now();
    let mut tasks = tokio::task::JoinSet::new();
    let mut running = std::collections::BTreeSet::new();
    let mut interval = tokio::time::interval(Duration::from_secs(2));
    loop {
        let remaining = seconds.saturating_sub(started.elapsed().as_secs());
        if *stop.borrow() || remaining < 25 {
            break;
        }
        tokio::select! {_=stop.changed()=>break,Some(result)=tasks.join_next(),if !tasks.is_empty()=>{if let Ok(key)=result{running.remove(&key);}},_=interval.tick()=>{
         let config=registry.configuration();if config.bindings.is_empty(){continue;}
         let mut after=String::new();
         'pages: while let Ok(page)=hub.goals("","collaboration",&after,true).await {
          let goals:Vec<Goal>=serde_json::from_value(page["goals"].clone()).unwrap_or_default();
          for goal in goals{for work in &goal.work {if tasks.len()>=4{break 'pages;}let key=(goal.input.id.clone(),work.id.clone(),work.attempt);if work.state!=WorkState::Ready || running.contains(&key) || !config.bindings.contains_key(&work.person_id){continue;}
           running.insert(key.clone());let hub=hub.clone();let config=config.clone();let work=work.clone();let goal=goal.clone();tasks.spawn(async move {let _=advance(hub,config,goal,work,remaining).await;key});
          }}
          match page["next_after"].as_str(){Some(next)=>after=next.into(),None=>break}
         }
        }}
    }
    tasks.shutdown().await;
}
