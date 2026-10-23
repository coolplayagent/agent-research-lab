//! Restartable scheduling of leased tasks through the replaceable execution protocol.
use crate::*;
use collaboration_model::*;
use contracts::{ServiceKind, TaskErrorCode as Code};
use service_api::{Adapter, AgentInput, Configuration, ExecutionReceipt, ExecutorRequest};
use service_registry::Registry;
use std::collections::{BTreeMap, BTreeSet};
async fn read(hub: &collaboration_client::Client, id: &str) -> Result<OutcomeTask> {
    Ok(serde_json::from_value(
        hub.tasks(TaskCommand::Get { id: id.into() }).await?,
    )?)
}
async fn change(
    hub: &collaboration_client::Client,
    task: &OutcomeTask,
    action: TaskAction,
) -> Result<OutcomeTask> {
    Ok(serde_json::from_value(
        hub.tasks(TaskCommand::Change {
            id: task.input.id.clone(),
            revision: task.revision,
            action,
        })
        .await?,
    )?)
}
async fn commit(
    hub: &collaboration_client::Client,
    id: &str,
    action: TaskAction,
) -> Result<OutcomeTask> {
    for _ in 0..8 {
        let task = read(hub, id).await?;
        match change(hub, &task, action.clone()).await {
            Ok(t) => return Ok(t),
            Err(e) if e.to_string().contains(Code::Conflict.as_str()) => continue,
            Err(e) => return Err(e),
        }
    }
    anyhow::bail!("{}", Code::Conflict)
}
async fn people(
    hub: &collaboration_client::Client,
    config: &Configuration,
    group: &str,
) -> Result<Value> {
    let mut people = vec![];
    let mut after = String::new();
    loop {
        let page = hub.members(group, &after, 100).await?;
        for person in page["members"].as_array().into_iter().flatten() {
            if let Some(id) = person["id"]
                .as_str()
                .or_else(|| person["person_id"].as_str())
                && worker_adapter(config, id).is_ok()
            {
                people.push(json!({"id":id,"name":hub.person(id).await?.name}));
            }
        }
        match page["next_after"].as_str() {
            Some(next) => after = next.into(),
            None => break,
        }
    }
    Ok(json!(people))
}
fn worker_adapter(config: &Configuration, id: &str) -> Result<Adapter> {
    let adapter_id = config.bindings.get(id).context(Code::Unavailable)?;
    let adapter = config
        .adapters
        .iter()
        .find(|a| a.id == *adapter_id && a.enabled)
        .context(Code::Unavailable)?;
    let evidence = config.binding_evidence.get(id).context(Code::Unavailable)?;
    ensure!(
        storage::digest(&serde_json::to_vec(adapter)?) == evidence.adapter_digest,
        "{}",
        Code::Unavailable
    );
    Ok(adapter.clone())
}
pub(crate) async fn execute(
    registry: &Registry,
    config: &Configuration,
    adapter: Adapter,
    input: AgentInput,
    seconds: u64,
) -> Result<ExecutionReceipt> {
    registry
        .prepare_execution(config, &adapter.executor_id)
        .await?;
    let executor = config.service(&adapter.executor_id, ServiceKind::Executor)?;
    let sandbox = config.service(
        executor.sandbox_id.as_deref().context(Code::Unavailable)?,
        ServiceKind::Sandbox,
    )?;
    let seconds = seconds.min(adapter.max_seconds);
    let receipt: ExecutionReceipt = service_protocol::call(
        &executor.endpoint,
        &ExecutorRequest::Execute {
            adapter: adapter.clone(),
            sandbox: sandbox.endpoint.clone(),
            input: Box::new(input.clone()),
            seconds,
        },
        Duration::from_secs(seconds + 15),
    )
    .await?;
    receipt.output.validate(&input)?;
    ensure!(
        receipt.adapter_id == adapter.id
            && receipt.agent == adapter.agent
            && receipt.input_sha256 == storage::digest(&serde_json::to_vec(&input)?)
            && receipt.adapter_sha256 == storage::digest(&serde_json::to_vec(&adapter)?)
            && receipt.output_sha256 == storage::digest(&serde_json::to_vec(&receipt.output)?),
        "{}",
        Code::Protocol
    );
    Ok(receipt)
}
async fn advance(
    hub: collaboration_client::Client,
    registry: Arc<Registry>,
    task: OutcomeTask,
    node_id: Option<String>,
) -> Result<()> {
    if node_id.is_none()
        && let Err(error) = probe(&registry, false).await
    {
        registry.record_task_diagnostic(&task.input.id, "coordinator-probe", &error, None);
        let _ = commit(
            &hub,
            &task.input.id,
            TaskAction::Pause {
                code: if error.to_string().contains(Code::Protocol.as_str()) {
                    Code::Protocol
                } else {
                    Code::Unavailable
                },
            },
        )
        .await;
        return Ok(());
    }
    let mut config = registry.configuration();
    let selection = if node_id.is_none() {
        registry.coordinator_configuration().and_then(|c| {
            let a = Registry::coordinator_adapter(&c)?;
            config = c;
            Ok(a)
        })
    } else {
        let node = task
            .nodes
            .iter()
            .find(|n| Some(&n.input.id) == node_id.as_ref())
            .context(Code::Invalid)?;
        worker_adapter(&config, &node.input.person_id)
    };
    let adapter = match selection {
        Ok(a) => a,
        Err(_) => {
            let _ = commit(
                &hub,
                &task.input.id,
                TaskAction::Pause {
                    code: Code::Unavailable,
                },
            )
            .await;
            return Ok(());
        }
    };
    if let Some(id) = &node_id {
        let assigned = &task
            .nodes
            .iter()
            .find(|n| n.input.id == *id)
            .context(Code::Invalid)?
            .input
            .person_id;
        let available = people(&hub, &config, &task.input.group_id).await?;
        if !available
            .as_array()
            .is_some_and(|p| p.iter().any(|p| p["id"].as_str() == Some(assigned)))
        {
            let _ = commit(
                &hub,
                &task.input.id,
                TaskAction::Pause {
                    code: Code::Unavailable,
                },
            )
            .await;
            return Ok(());
        }
    }
    let mut random = [0u8; 32];
    getrandom::fill(&mut random)?;
    let claim_id = storage::digest(&random);
    let task = change(
        &hub,
        &task,
        TaskAction::Claim {
            node_id: node_id.clone(),
            claim_id: claim_id.clone(),
        },
    )
    .await?;
    let mut diagnostic_receipt = None;
    let outcome=async {
        let instruction=if let Some(id)=&node_id {crate::task_prompt::worker(&task,task.nodes.iter().find(|n|n.input.id==*id).unwrap())}else{let members=people(&hub,&config,&task.input.group_id).await?;if task.kind == contracts::TaskKind::Message { let candidates = routing_candidates(&hub, &task).await?; crate::task_prompt::routing(&task, &members, &candidates) } else { ensure!(members.as_array().is_some_and(|p|!p.is_empty()),"{}",Code::Unavailable);crate::task_prompt::coordinator(&task,&members) }};
        let node=node_id.as_ref().and_then(|id|task.nodes.iter().find(|n|n.input.id==*id));
        let input=AgentInput{protocol_version:service_protocol::VERSION,request_id:claim_id.clone(),person_id:node.map_or("coordinator",|n|n.input.person_id.as_str()).into(),goal_id:task.input.id.clone(),work_id:node_id.clone().unwrap_or_else(||"coordinator".into()),attempt:node.map_or(task.coordinator_calls,|n|n.attempt),objective:node.map_or_else(|| "Propose the next coordination action for the supplied user intent".into(), |_| task.input.intent.clone()),acceptance:node.map_or("Return one valid coordinator proposal. This turn succeeds when that proposal is ready for host validation; the overall user outcome may still be pending.",|n|n.input.acceptance.as_str()).into(),instruction,messages:hub.history_before(&task.input.group_id,i64::MAX as u64,16).await?};
        ensure!(input.instruction.len()<=1024*1024,"{}",Code::Budget);
        let execution=execute(&registry,&config,adapter.clone(),input,task.input.budget.execution_seconds);tokio::pin!(execution);
        let mut poll=tokio::time::interval(Duration::from_secs(2));
        let receipt=loop {tokio::select!{r=&mut execution=>break r?,_=poll.tick()=>{let current=read(&hub,&task.input.id).await?;let current_config=registry.configuration();ensure!(current.active() && current.graph_revision==task.graph_revision,"{}",Code::StaleAttempt);let lease=node_id.as_ref().and_then(|id|current.nodes.iter().find(|n|n.input.id==*id)).and_then(|n|n.lease.as_ref()).or_else(||if node_id.is_none(){current.coordinator_lease.as_ref()}else{None});ensure!(lease.is_some_and(|l|l.claim_id==claim_id && l.deadline_ms>now_ms()),"{}",Code::StaleAttempt);let current_adapter=if node_id.is_none(){Registry::coordinator_adapter(&current_config)?}else{worker_adapter(&current_config,&node.unwrap().input.person_id)?};ensure!(serde_json::to_value(&adapter)?==serde_json::to_value(current_adapter)?,"{}",Code::Unavailable);}}};
        diagnostic_receipt = Some(receipt.clone());
        if let Some(node_id)=&node_id {Ok(TaskAction::Submit{node_id:node_id.clone(),claim_id:claim_id.clone(),summary:receipt.output.summary.clone(),evidence:receipt.output.evidence.clone(),succeeded:receipt.output.succeeded,receipt:json!(receipt)})}else{
            let output = coordinator_output(&receipt)?;
            Ok(TaskAction::Coordinate{claim_id:claim_id.clone(),output,receipt:json!(receipt)})
        }
    }.await;
    let probe = match &outcome {
        Ok(TaskAction::Coordinate { receipt, .. })
            if config
                .coordinator
                .as_ref()
                .is_some_and(|c| c.protocol_evidence.is_none()) =>
        {
            receipt["output_sha256"].as_str().map(str::to_owned)
        }
        _ => None,
    };
    let action = outcome.unwrap_or_else(|e| {
        registry.record_task_diagnostic(&task.input.id, &claim_id, &e, diagnostic_receipt.as_ref());
        TaskAction::Fail {
            node_id: node_id.clone(),
            claim_id: claim_id.clone(),
            code: if e.to_string().contains(Code::Protocol.as_str()) {
                Code::Protocol
            } else {
                Code::Unavailable
            },
        }
    });
    if let Err(e) = commit(&hub, &task.input.id, action).await {
        registry.record_task_diagnostic(
            &task.input.id,
            &claim_id,
            &anyhow::anyhow!("Storage rejected execution result: {e:#}"),
            diagnostic_receipt.as_ref(),
        );
        let code = if e.to_string().contains(Code::Conflict.as_str()) {
            Code::Unavailable
        } else if e.to_string().contains(Code::Budget.as_str()) {
            Code::Budget
        } else {
            Code::Protocol
        };
        let _ = commit(
            &hub,
            &task.input.id,
            TaskAction::Fail {
                node_id,
                claim_id,
                code,
            },
        )
        .await;
    } else if let Some(digest) = probe {
        let _ = registry.record_coordinator_probe(&config, digest);
    }
    Ok(())
}
pub(crate) async fn run(
    hub: collaboration_client::Client,
    registry: Arc<Registry>,
    seconds: u64,
    mut stop: tokio::sync::watch::Receiver<bool>,
) {
    let started = std::time::Instant::now();
    let mut jobs = tokio::task::JoinSet::new();
    let mut running = BTreeSet::new();
    let mut last_ticks = BTreeMap::new();
    let mut interval = tokio::time::interval(Duration::from_secs(2));
    loop {
        if *stop.borrow() || started.elapsed().as_secs() + 25 >= seconds {
            break;
        }
        tokio::select! {_=stop.changed()=>break,Some(Ok(key))=jobs.join_next(),if !jobs.is_empty()=>{running.remove(&key);},_=interval.tick()=>{
            let mut after=String::new();'pages: loop {let Ok(page)=hub.tasks(TaskCommand::List{group_id:String::new(),after:after.clone(),active:true,questions:true}).await else{break;};let tasks:Vec<OutcomeTask>=serde_json::from_value(page["tasks"].clone()).unwrap_or_default();
            for mut task in tasks {
                if now_ms().saturating_sub(*last_ticks.get(&task.input.id).unwrap_or(&0))>5000 {if let Ok(updated)=change(&hub,&task,TaskAction::Tick).await{task=updated;}last_ticks.insert(task.input.id.clone(),now_ms());}
                if !task.active(){continue;}
                let ids:Vec<Option<String>>=if task.needs_coordinator(){vec![None]}else{task.nodes.iter().filter(|n|task.ready(n)).map(|n|Some(n.input.id.clone())).collect()};
                for node_id in ids {if jobs.len()>=4{break 'pages;}let key=(task.input.id.clone(),node_id.clone());if !running.insert(key.clone()){continue;}let hub=hub.clone();let registry=registry.clone();let task=task.clone();jobs.spawn(async move{let _=advance(hub,registry,task,node_id).await;key});}
            }
            match page["next_after"].as_str(){Some(next)=>after=next.into(),None=>break}
            }
        }}
    }
    jobs.shutdown().await;
}

pub(crate) async fn probe(registry: &Registry, force: bool) -> Result<Configuration> {
    let _lock = registry.coordinator_probe_lock().await;
    let config = registry.coordinator_configuration()?;
    if !force && Registry::coordinator_probe_valid(&config) {
        return Ok(config);
    }
    let adapter = Registry::coordinator_adapter(&config)?;
    let mut random = [0u8; 32];
    getrandom::fill(&mut random)?;
    let request_id = storage::digest(&random);
    let input=AgentInput{protocol_version:service_protocol::VERSION,request_id,person_id:"coordinator".into(),goal_id:"coordinator-probe".into(),work_id:"protocol-check".into(),attempt:1,objective:"Check the OutcomeHub coordinator protocol".into(),acceptance:"Return the exact typed proposal".into(),instruction:"This is a bounded protocol check, not a user task. Do not execute tools or modify files. Return succeeded=true, summary=Coordinator protocol ready, and evidence={\"coordinator\":{\"protocol_version\":1,\"proposal\":{\"action\":\"clarify\",\"question\":\"What outcome should the team deliver?\"}}}.".into(),messages:vec![]};
    let receipt = execute(registry, &config, adapter, input, 60).await?;
    let output = coordinator_output(&receipt)?;
    ensure!(
        output.protocol_version == COORDINATOR_PROTOCOL_VERSION
            && matches!(output.proposal, CoordinatorProposal::Clarify { .. }),
        "{}",
        Code::Protocol
    );
    registry.record_coordinator_probe(&config, receipt.output_sha256)?;
    Ok(registry.configuration())
}

async fn routing_candidates(
    hub: &collaboration_client::Client,
    task: &OutcomeTask,
) -> Result<Value> {
    let page = hub
        .tasks(TaskCommand::List {
            group_id: task.input.group_id.clone(),
            after: String::new(),
            active: false,
            questions: true,
        })
        .await?;
    let mut candidates: Vec<OutcomeTask> = serde_json::from_value(page["tasks"].clone())?;
    if let Some(id) = &task.reply_task_id
        && !candidates.iter().any(|t| &t.input.id == id)
    {
        let target = read(hub, id).await?;
        ensure!(
            target.input.group_id == task.input.group_id,
            "{}",
            Code::Forbidden
        );
        candidates.push(target);
    }
    Ok(
        json!({"has_older":!page["next_after"].is_null(),"tasks":candidates.iter().filter(|t|t.input.id!=task.input.id && t.kind!=contracts::TaskKind::Question && (t.kind==contracts::TaskKind::Outcome || t.state==contracts::TaskState::WaitingInput)).map(|t|json!({"id":t.input.id,"revision":t.revision,"kind":t.kind,"state":t.state,"intent":t.input.intent.chars().take(1000).collect::<String>(),"title":t.title,"question":t.question,"latest_note":t.notes.last(),"progress":t.nodes.iter().map(|n|json!({"title":n.input.title,"state":n.state})).collect::<Vec<_>>()})).collect::<Vec<_>>()}),
    )
}

/// Every response is validated, even after a successful configuration probe.
fn coordinator_output(receipt: &ExecutionReceipt) -> Result<CoordinatorOutput> {
    let result = (|| {
        ensure!(
            receipt.output.succeeded,
            "Coordinator reported succeeded=false"
        );
        let output: CoordinatorOutput =
            serde_json::from_value(receipt.output.evidence["coordinator"].clone())
                .context("Invalid evidence.coordinator")?;
        ensure!(
            output.protocol_version == COORDINATOR_PROTOCOL_VERSION,
            "Unsupported coordinator protocol version: {}",
            output.protocol_version
        );
        Ok(output)
    })();
    result.context(Code::Protocol)
}

#[cfg(test)]
#[path = "task_execution_tests.rs"]
mod tests;
