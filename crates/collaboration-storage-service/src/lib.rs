//! Durable collaboration storage client and independently hosted service.
use anyhow::{Result, ensure};
pub use collaboration_client::{Client, Request};
use collaboration_store::*;
use rpc::Stream;
use serde_json::{Value, json};
use service_protocol as rpc;
use std::{path::Path, sync::Arc, time::Duration};
/// Explicit development adapter; the host chooses an independent local state directory.
pub fn client(hub: Hub) -> Client {
    Client::Local(Arc::new(LocalStore(hub)))
}
struct LocalStore(Hub);
impl collaboration_client::EmbeddedStorage for LocalStore {
    fn request(&self, request: Request) -> collaboration_client::LocalFuture<'_, Value> {
        Box::pin(execute(&self.0, request))
    }
    fn changes(&self) -> tokio::sync::watch::Receiver<u64> {
        self.0.changes()
    }
    fn subscribe(
        &self,
        actor: &str,
        group: &str,
    ) -> Result<Box<dyn collaboration_client::LocalSubscription>> {
        Ok(Box::new(LocalEvents(self.0.subscribe(actor, group)?)))
    }
}
struct LocalEvents(collaboration_store::Subscription);
impl collaboration_client::LocalSubscription for LocalEvents {
    fn receives(&self) -> Result<bool> {
        self.0.receives()
    }
    fn recv(&mut self) -> collaboration_client::LocalFuture<'_, Option<Arc<Message>>> {
        Box::pin(self.0.recv())
    }
    fn wait_control(&mut self) -> collaboration_client::LocalFuture<'_, ()> {
        Box::pin(self.0.wait_control())
    }
    fn replayed(&self, count: usize) {
        self.0.replayed(count);
    }
}
fn info() -> rpc::ServiceInfo {
    rpc::ServiceInfo {
        protocol_version: rpc::VERSION,
        service_id: "ai-im-storage".into(),
        kind: contracts::ServiceKind::Storage,
        capabilities: vec!["collaboration.v1".into(), "events.v1".into()],
        ready: true,
    }
}
async fn execute(hub: &Hub, request: Request) -> Result<Value> {
    match request {
        Request::Task { command } => hub.tasks(*command).await,
        Request::Outcomes { token, command } => hub.outcomes(token.as_deref(), *command).await,
        Request::Probe => Ok(json!(info())),
        Request::Metrics => Ok(hub.metrics()),
        Request::Receives { actor, group } => {
            hub.authorize(&actor, &group)?;
            ensure!(hub.metrics()["healthy"] == true, "durable writer fault");
            Ok(json!(hub.presence(&actor)?.presence.receives()))
        }
        Request::Register { actors } => Ok(json!(hub.register(actors).await?)),
        Request::SavePerson { person } => Ok(json!(hub.save_person(person).await?)),
        Request::ArchiveUnbound { retained } => Ok(json!(hub.archive_unbound(retained).await?)),
        Request::RegisterOperator {} => Ok(json!(hub.register_operator().await?)),
        Request::Convert {
            group,
            revision,
            title,
        } => Ok(json!(hub.convert(&group, revision, title).await?)),
        Request::Fork {
            source,
            group,
            history_from,
        } => Ok(json!(hub.fork(&source, group, history_from).await?)),
        Request::CreateGroup { group } => Ok(json!(hub.create_group(group).await?)),
        Request::Delete { target } => Ok(json!(hub.delete(target).await?)),
        Request::UpdateGroup { change } => Ok(json!(hub.update_group(change).await?)),
        Request::Membership { group, actor, add } => {
            Ok(json!(hub.membership(&group, &actor, add).await?))
        }
        Request::SetPresence { actor, state } => Ok(json!(hub.set_presence(&actor, state).await?)),
        Request::Grant {
            actor,
            lifetime_seconds,
        } => Ok(json!(hub.grant(&actor, lifetime_seconds).await?)),
        Request::Publish { token, input } => Ok(json!(hub.publish(&token, input).await?)),
        Request::PublishAs { actor, input } => Ok(json!(hub.publish_as(&actor, input).await?)),
        Request::History {
            group,
            after,
            limit,
        } => Ok(json!(hub.history(&group, after, limit).await?)),
        Request::HistoryBefore {
            group,
            before,
            limit,
        } => Ok(json!(hub.history_before(&group, before, limit).await?)),
        Request::Acknowledge {
            token,
            group,
            sequence,
        } => Ok(json!(hub.acknowledge(&token, &group, sequence).await?)),
        Request::Cursor { actor, group } => Ok(json!(hub.cursor(&actor, &group).await?)),
        Request::Discuss { input, message } => hub.discuss(input, message).await,
        Request::Goal { id } => Ok(json!(hub.goal(&id).await?)),
        Request::ChangeGoal { change } => Ok(json!(hub.change_goal(change).await?)),
        Request::Work { token, change } => Ok(json!(hub.work(&token, change).await?)),
        Request::WorkAs { actor, change } => Ok(json!(hub.work_as(&actor, change).await?)),
        Request::Goals {
            group,
            scenario,
            after,
            active,
        } => Ok(json!(hub.goals(&group, &scenario, &after, active).await?)),
        Request::CurrentGoal { group } => hub.current_goal(&group).await,
        Request::AssignedGoals { token, after } => {
            Ok(json!(hub.assigned_goals(&token, &after).await?))
        }
        Request::Authenticate { token } => Ok(json!(hub.authenticate(&token)?)),
        Request::GrantExpiry { token } => Ok(json!(hub.grant_expiry(&token)?)),
        Request::Authorize { actor, group } => Ok(json!(hub.authorize(&actor, &group)?)),
        Request::Groups {
            after,
            query,
            limit,
        } => Ok(json!(hub.groups(&after, &query, limit)?)),
        Request::Group { group } => Ok(json!(hub.group(&group)?)),
        Request::Members {
            group,
            after,
            limit,
        } => Ok(json!(hub.members(&group, &after, limit)?)),
        Request::Presence { actor } => Ok(json!(hub.presence(&actor)?)),
        Request::People {
            after,
            query,
            limit,
        } => Ok(json!(hub.people(&after, &query, limit)?)),
        Request::Inbox {
            token,
            after,
            limit,
        } => Ok(json!(hub.inbox(&token, &after, limit)?)),
        Request::Person { actor } => Ok(json!(hub.person(&actor)?)),
        Request::Changes | Request::Subscribe { .. } => {
            anyhow::bail!("stream request requires persistent connection")
        }
    }
}
async fn reply(stream: &mut Stream, value: Result<Value>) -> Result<()> {
    tokio::time::timeout(
        Duration::from_secs(5),
        rpc::write(stream, &rpc::Reply::from_result(value)),
    )
    .await?
}
async fn disconnected(stream: &Stream) {
    use tokio::io::Interest;
    loop {
        if stream.ready(Interest::READABLE).await.is_err() {
            return;
        }
        match stream.try_read(&mut [0u8; 1]) {
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
            _ => return,
        }
    }
}
async fn connection(hub: Hub, mut stream: Stream) -> Result<()> {
    let envelope: rpc::Envelope<Request> =
        tokio::time::timeout(Duration::from_secs(5), rpc::read(&mut stream)).await??;
    ensure!(
        envelope.protocol_version == rpc::VERSION,
        "unsupported storage protocol version"
    );
    match envelope.request {
        Request::Changes => {
            let mut changes = hub.changes();
            reply(&mut stream, Ok(Value::Null)).await?;
            loop {
                tokio::select! {_=disconnected(&stream)=>return Ok(()),changed=changes.changed()=>changed?};
                reply(&mut stream, Ok(Value::Null)).await?;
            }
        }
        Request::Subscribe { actor, group } => {
            let mut sub = match hub.subscribe(&actor, &group) {
                Ok(sub) => sub,
                Err(e) => return reply(&mut stream, Err(e)).await,
            };
            reply(&mut stream, Ok(Value::Null)).await?;
            loop {
                let message = tokio::select! {_=disconnected(&stream)=>return Ok(()),message=sub.recv()=>message?};
                reply(&mut stream, Ok(json!(message.as_deref()))).await?;
            }
        }
        request => {
            reply(
                &mut stream,
                tokio::time::timeout(Duration::from_secs(10), execute(&hub, request)).await?,
            )
            .await
        }
    }
}
pub async fn serve(state: &Path, socket: &Path, seconds: u64) -> Result<()> {
    ensure!(
        (1..=43200).contains(&seconds),
        "service lifetime must be 1..43200 seconds"
    );
    let hub = Hub::open(state)?;
    let listener = rpc::Listener::bind(socket)?;
    let permits = Arc::new(tokio::sync::Semaphore::new(20132));
    let mut tasks = tokio::task::JoinSet::new();
    let expires = tokio::time::sleep(Duration::from_secs(seconds));
    tokio::pin!(expires);
    loop {
        tokio::select! {_=&mut expires=>break,Some(_)=tasks.join_next(),if !tasks.is_empty()=>{},stream=listener.accept()=>{let stream=stream?;if let Ok(permit)=permits.clone().try_acquire_owned(){let hub=hub.clone();tasks.spawn(async move{let _permit=permit;let _=connection(hub,stream).await;});}}}
    }
    tasks.shutdown().await;
    Ok(())
}
#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
