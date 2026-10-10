//! Stateless executor and lightweight sandbox services; AI-IM owns goals and storage.
mod sandbox;
use anyhow::{Result, ensure};
use contracts::ServiceKind;
pub use sandbox::run;
use serde_json::{Value, json};
use service_api::{ExecutionReceipt, ExecutorRequest, SandboxRequest};
use service_protocol as rpc;
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};
async fn execute(request: ExecutorRequest) -> Result<Value> {
    match request {
        ExecutorRequest::Probe => Ok(json!(info(ServiceKind::Executor, true))),
        ExecutorRequest::Execute {
            adapter,
            sandbox,
            input,
            seconds,
        } => {
            adapter.validate()?;
            ensure!(adapter.enabled, "adapter is disabled");
            ensure!(
                input.protocol_version == rpc::VERSION,
                "unsupported agent input protocol version"
            );
            ensure!(
                (1..=adapter.max_seconds).contains(&seconds),
                "execution time exceeds adapter limit"
            );
            rpc::endpoint(&sandbox)?;
            service_api::probe(&sandbox, ServiceKind::Sandbox).await?;
            let encoded = serde_json::to_string(&input)?;
            ensure!(encoded.len() <= 256 * 1024, "agent input exceeds bound");
            let input_sha256 = storage::digest(encoded.as_bytes());
            let adapter_sha256 = storage::digest(&serde_json::to_vec(&adapter)?);
            let output: service_api::SandboxOutput = rpc::call(
                &sandbox,
                &SandboxRequest::Run {
                    command: adapter.command.clone(),
                    env_names: adapter.env_names.clone(),
                    read_only_paths: adapter.read_only_paths.clone(),
                    network: adapter.network,
                    input: encoded,
                    seconds,
                },
                Duration::from_secs(seconds + 10),
            )
            .await?;
            let result: service_api::AgentOutput = serde_json::from_str(&output.stdout)?;
            result.validate(&input)?;
            Ok(json!(ExecutionReceipt {
                output_sha256: storage::digest(&serde_json::to_vec(&result)?),
                output: result,
                agent: adapter.agent,
                adapter_id: adapter.id,
                input_sha256,
                adapter_sha256,
                sandbox_id: output.sandbox_id,
                elapsed_ms: output.elapsed_ms
            }))
        }
    }
}
fn info(kind: ServiceKind, ready: bool) -> rpc::ServiceInfo {
    rpc::ServiceInfo {
        protocol_version: rpc::VERSION,
        service_id: format!("ai-im-{kind}"),
        kind,
        capabilities: vec![
            match kind {
                ServiceKind::Executor => "json_stdio.v1",
                ServiceKind::Sandbox => "sandbox.run.v1",
                ServiceKind::Storage => unreachable!(),
            }
            .into(),
        ],
        ready,
    }
}
async fn handle(
    kind: ServiceKind,
    mut stream: tokio::net::UnixStream,
    state: &Path,
    remaining: u64,
) -> Result<()> {
    let envelope: rpc::Envelope<Value> =
        tokio::time::timeout(Duration::from_secs(5), rpc::read(&mut stream)).await??;
    ensure!(
        envelope.protocol_version == rpc::VERSION,
        "unsupported service protocol version"
    );
    let action = async {
        match kind {
            ServiceKind::Executor => execute(serde_json::from_value(envelope.request)?).await,
            ServiceKind::Sandbox => match serde_json::from_value(envelope.request)? {
                SandboxRequest::Probe => {
                    sandbox::probe(state).await?;
                    Ok(json!(info(kind, true)))
                }
                SandboxRequest::Run {
                    command,
                    env_names,
                    read_only_paths,
                    network,
                    input,
                    seconds,
                } => {
                    ensure!(
                        seconds + 5 < remaining,
                        "sandbox service lifetime is too short for this execution"
                    );
                    Ok(json!(
                        run(
                            state,
                            command,
                            env_names,
                            read_only_paths,
                            network,
                            input,
                            seconds
                        )
                        .await?
                    ))
                }
            },
            ServiceKind::Storage => anyhow::bail!("use the storage service binary"),
        }
    };
    let result = tokio::select! {result=tokio::time::timeout(Duration::from_secs(remaining),action)=>result?,_=disconnected(&stream)=>return Ok(())};
    tokio::time::timeout(
        Duration::from_secs(5),
        rpc::write(&mut stream, &rpc::Reply::from_result(result)),
    )
    .await?
}
async fn disconnected(stream: &tokio::net::UnixStream) {
    loop {
        if stream.readable().await.is_err() {
            return;
        }
        match stream.try_read(&mut [0u8; 1]) {
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
            _ => return,
        }
    }
}
pub async fn serve(
    kind: ServiceKind,
    state: &Path,
    socket: &Path,
    seconds: u64,
    capacity: usize,
) -> Result<()> {
    ensure!(
        kind != ServiceKind::Storage
            && (1..=43200).contains(&seconds)
            && (1..=32).contains(&capacity),
        "invalid service kind, lifetime or capacity"
    );
    std::fs::create_dir_all(state)?;
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(state, std::fs::Permissions::from_mode(0o700))?;
    let listener = rpc::Listener::bind(socket)?;
    let permits = Arc::new(tokio::sync::Semaphore::new(capacity));
    let mut tasks = tokio::task::JoinSet::new();
    let started = Instant::now();
    let expires = tokio::time::sleep(Duration::from_secs(seconds));
    tokio::pin!(expires);
    loop {
        tokio::select! {_=&mut expires=>break,Some(_)=tasks.join_next(),if !tasks.is_empty()=>{},stream=listener.accept()=>{let mut stream=stream?;match permits.clone().try_acquire_owned(){Ok(permit)=>{let state=state.to_owned();let remaining=seconds.saturating_sub(started.elapsed().as_secs());tasks.spawn(async move {let _permit=permit;let _=handle(kind,stream,&state,remaining).await;});},Err(_)=>{let _=tokio::time::timeout(Duration::from_secs(1),rpc::write(&mut stream,&rpc::Reply::from_result(Err(anyhow::anyhow!("service capacity backpressure"))))).await;}}}}
    }
    tasks.shutdown().await;
    Ok(())
}
#[cfg(test)]
mod tests;
