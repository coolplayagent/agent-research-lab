//! Durable collaboration storage client and independently hosted service.
use anyhow::{Result, ensure};
use crystal::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use service_protocol as rpc;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::net::UnixStream;
#[derive(Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    Probe,
    Metrics,
    Changes,
    Subscribe {
        actor: String,
        group: String,
    },
    Receives {
        actor: String,
        group: String,
    },
    Register {
        actors: Vec<String>,
    },
    SavePerson {
        person: Person,
    },
    RegisterOperator {},
    Convert {
        group: String,
        revision: u64,
        title: String,
    },
    Fork {
        source: String,
        group: NewGroup,
        history_from: Option<u64>,
    },
    CreateGroup {
        group: NewGroup,
    },
    UpdateGroup {
        change: GroupChange,
    },
    Membership {
        group: String,
        actor: String,
        add: bool,
    },
    SetPresence {
        actor: String,
        state: Presence,
    },
    Grant {
        actor: String,
        lifetime_seconds: u64,
    },
    Publish {
        token: String,
        input: Publish,
    },
    PublishAs {
        actor: String,
        input: Publish,
    },
    History {
        group: String,
        after: u64,
        limit: usize,
    },
    HistoryBefore {
        group: String,
        before: u64,
        limit: usize,
    },
    Acknowledge {
        token: String,
        group: String,
        sequence: u64,
    },
    Cursor {
        actor: String,
        group: String,
    },
    ChangeGoal {
        change: GoalChange,
    },
    Work {
        token: String,
        change: WorkChange,
    },
    WorkAs {
        actor: String,
        change: WorkChange,
    },
    Goals {
        group: String,
        scenario: String,
        after: String,
        active: bool,
    },
    AssignedGoals {
        token: String,
        after: String,
    },
    Authenticate {
        token: String,
    },
    GrantExpiry {
        token: String,
    },
    Authorize {
        actor: String,
        group: String,
    },
    Groups {
        after: String,
        query: String,
        limit: usize,
    },
    Group {
        group: String,
    },
    Members {
        group: String,
        after: String,
        limit: usize,
    },
    Presence {
        actor: String,
    },
    People {
        after: String,
        query: String,
        limit: usize,
    },
    Inbox {
        token: String,
        after: String,
        limit: usize,
    },
    Person {
        actor: String,
    },
}
#[derive(Clone)]
pub enum Client {
    Local(Hub),
    Remote(PathBuf),
}
impl From<Hub> for Client {
    fn from(hub: Hub) -> Self {
        Self::Local(hub)
    }
}
impl Client {
    pub fn remote(path: PathBuf) -> Result<Self> {
        rpc::endpoint(&path)?;
        Ok(Self::Remote(path))
    }
    pub async fn probe(&self) -> Result<rpc::ServiceInfo> {
        match self {
            Self::Local(_) => Ok(info()),
            Self::Remote(path) => rpc::call(path, &Request::Probe, Duration::from_secs(5)).await,
        }
    }
    pub async fn metrics(&self) -> Result<Value> {
        match self {
            Self::Local(hub) => Ok(hub.metrics()),
            Self::Remote(path) => rpc::call(path, &Request::Metrics, Duration::from_secs(10)).await,
        }
    }
    pub async fn register(&self, actors: Vec<String>) -> Result<()> {
        match self {
            Self::Local(hub) => hub.register(actors).await,
            Self::Remote(path) => {
                rpc::call(path, &Request::Register { actors }, Duration::from_secs(10)).await
            }
        }
    }
    pub async fn save_person(&self, person: Person) -> Result<()> {
        match self {
            Self::Local(hub) => hub.save_person(person).await,
            Self::Remote(path) => {
                rpc::call(
                    path,
                    &Request::SavePerson { person },
                    Duration::from_secs(10),
                )
                .await
            }
        }
    }
    pub async fn register_operator(&self) -> Result<()> {
        match self {
            Self::Local(hub) => hub.register_operator().await,
            Self::Remote(path) => {
                rpc::call(path, &Request::RegisterOperator {}, Duration::from_secs(10)).await
            }
        }
    }
    pub async fn convert(&self, group: &str, revision: u64, title: String) -> Result<Group> {
        match self {
            Self::Local(hub) => hub.convert(group, revision, title).await,
            Self::Remote(path) => {
                rpc::call(
                    path,
                    &Request::Convert {
                        group: group.into(),
                        revision,
                        title,
                    },
                    Duration::from_secs(10),
                )
                .await
            }
        }
    }
    pub async fn fork(
        &self,
        source: &str,
        group: NewGroup,
        history_from: Option<u64>,
    ) -> Result<Group> {
        match self {
            Self::Local(hub) => hub.fork(source, group, history_from).await,
            Self::Remote(path) => {
                rpc::call(
                    path,
                    &Request::Fork {
                        source: source.into(),
                        group,
                        history_from,
                    },
                    Duration::from_secs(10),
                )
                .await
            }
        }
    }
    pub async fn create_group(&self, group: NewGroup) -> Result<Group> {
        match self {
            Self::Local(hub) => hub.create_group(group).await,
            Self::Remote(path) => {
                rpc::call(
                    path,
                    &Request::CreateGroup { group },
                    Duration::from_secs(10),
                )
                .await
            }
        }
    }
    pub async fn update_group(&self, change: GroupChange) -> Result<()> {
        match self {
            Self::Local(hub) => hub.update_group(change).await,
            Self::Remote(path) => {
                rpc::call(
                    path,
                    &Request::UpdateGroup { change },
                    Duration::from_secs(10),
                )
                .await
            }
        }
    }
    pub async fn membership(&self, group: &str, actor: &str, add: bool) -> Result<()> {
        match self {
            Self::Local(hub) => hub.membership(group, actor, add).await,
            Self::Remote(path) => {
                rpc::call(
                    path,
                    &Request::Membership {
                        group: group.into(),
                        actor: actor.into(),
                        add,
                    },
                    Duration::from_secs(10),
                )
                .await
            }
        }
    }
    pub async fn set_presence(&self, actor: &str, state: Presence) -> Result<()> {
        match self {
            Self::Local(hub) => hub.set_presence(actor, state).await,
            Self::Remote(path) => {
                rpc::call(
                    path,
                    &Request::SetPresence {
                        actor: actor.into(),
                        state,
                    },
                    Duration::from_secs(10),
                )
                .await
            }
        }
    }
    pub async fn grant(&self, actor: &str, lifetime_seconds: u64) -> Result<String> {
        match self {
            Self::Local(hub) => hub.grant(actor, lifetime_seconds).await,
            Self::Remote(path) => {
                rpc::call(
                    path,
                    &Request::Grant {
                        actor: actor.into(),
                        lifetime_seconds,
                    },
                    Duration::from_secs(10),
                )
                .await
            }
        }
    }
    pub async fn publish(&self, token: &str, input: Publish) -> Result<Receipt> {
        match self {
            Self::Local(hub) => hub.publish(token, input).await,
            Self::Remote(path) => {
                rpc::call(
                    path,
                    &Request::Publish {
                        token: token.into(),
                        input,
                    },
                    Duration::from_secs(10),
                )
                .await
            }
        }
    }
    pub async fn publish_as(&self, actor: &str, input: Publish) -> Result<Receipt> {
        match self {
            Self::Local(hub) => hub.publish_as(actor, input).await,
            Self::Remote(path) => {
                rpc::call(
                    path,
                    &Request::PublishAs {
                        actor: actor.into(),
                        input,
                    },
                    Duration::from_secs(10),
                )
                .await
            }
        }
    }
    pub async fn history(&self, group: &str, after: u64, limit: usize) -> Result<Vec<Message>> {
        match self {
            Self::Local(hub) => hub.history(group, after, limit).await,
            Self::Remote(path) => {
                rpc::call(
                    path,
                    &Request::History {
                        group: group.into(),
                        after,
                        limit,
                    },
                    Duration::from_secs(10),
                )
                .await
            }
        }
    }
    pub async fn history_before(
        &self,
        group: &str,
        before: u64,
        limit: usize,
    ) -> Result<Vec<Message>> {
        match self {
            Self::Local(hub) => hub.history_before(group, before, limit).await,
            Self::Remote(path) => {
                rpc::call(
                    path,
                    &Request::HistoryBefore {
                        group: group.into(),
                        before,
                        limit,
                    },
                    Duration::from_secs(10),
                )
                .await
            }
        }
    }
    pub async fn acknowledge(&self, token: &str, group: &str, sequence: u64) -> Result<()> {
        match self {
            Self::Local(hub) => hub.acknowledge(token, group, sequence).await,
            Self::Remote(path) => {
                rpc::call(
                    path,
                    &Request::Acknowledge {
                        token: token.into(),
                        group: group.into(),
                        sequence,
                    },
                    Duration::from_secs(10),
                )
                .await
            }
        }
    }
    pub async fn cursor(&self, actor: &str, group: &str) -> Result<u64> {
        match self {
            Self::Local(hub) => hub.cursor(actor, group).await,
            Self::Remote(path) => {
                rpc::call(
                    path,
                    &Request::Cursor {
                        actor: actor.into(),
                        group: group.into(),
                    },
                    Duration::from_secs(10),
                )
                .await
            }
        }
    }
    pub async fn change_goal(&self, change: GoalChange) -> Result<Goal> {
        match self {
            Self::Local(hub) => hub.change_goal(change).await,
            Self::Remote(path) => {
                rpc::call(
                    path,
                    &Request::ChangeGoal { change },
                    Duration::from_secs(10),
                )
                .await
            }
        }
    }
    pub async fn work(&self, token: &str, change: WorkChange) -> Result<Goal> {
        match self {
            Self::Local(hub) => hub.work(token, change).await,
            Self::Remote(path) => {
                rpc::call(
                    path,
                    &Request::Work {
                        token: token.into(),
                        change,
                    },
                    Duration::from_secs(10),
                )
                .await
            }
        }
    }
    pub async fn work_as(&self, actor: &str, change: WorkChange) -> Result<Goal> {
        match self {
            Self::Local(hub) => hub.work_as(actor, change).await,
            Self::Remote(path) => {
                rpc::call(
                    path,
                    &Request::WorkAs {
                        actor: actor.into(),
                        change,
                    },
                    Duration::from_secs(10),
                )
                .await
            }
        }
    }
    pub async fn goals(
        &self,
        group: &str,
        scenario: &str,
        after: &str,
        active: bool,
    ) -> Result<Value> {
        match self {
            Self::Local(hub) => hub.goals(group, scenario, after, active).await,
            Self::Remote(path) => {
                rpc::call(
                    path,
                    &Request::Goals {
                        group: group.into(),
                        scenario: scenario.into(),
                        after: after.into(),
                        active,
                    },
                    Duration::from_secs(10),
                )
                .await
            }
        }
    }
    pub async fn assigned_goals(&self, token: &str, after: &str) -> Result<Value> {
        match self {
            Self::Local(hub) => hub.assigned_goals(token, after).await,
            Self::Remote(path) => {
                rpc::call(
                    path,
                    &Request::AssignedGoals {
                        token: token.into(),
                        after: after.into(),
                    },
                    Duration::from_secs(10),
                )
                .await
            }
        }
    }
    pub async fn authenticate(&self, token: &str) -> Result<String> {
        match self {
            Self::Local(hub) => hub.authenticate(token),
            Self::Remote(path) => {
                rpc::call(
                    path,
                    &Request::Authenticate {
                        token: token.into(),
                    },
                    Duration::from_secs(10),
                )
                .await
            }
        }
    }
    pub async fn grant_expiry(&self, token: &str) -> Result<u64> {
        match self {
            Self::Local(hub) => hub.grant_expiry(token),
            Self::Remote(path) => {
                rpc::call(
                    path,
                    &Request::GrantExpiry {
                        token: token.into(),
                    },
                    Duration::from_secs(10),
                )
                .await
            }
        }
    }
    pub async fn authorize(&self, actor: &str, group: &str) -> Result<()> {
        match self {
            Self::Local(hub) => hub.authorize(actor, group),
            Self::Remote(path) => {
                rpc::call(
                    path,
                    &Request::Authorize {
                        actor: actor.into(),
                        group: group.into(),
                    },
                    Duration::from_secs(10),
                )
                .await
            }
        }
    }
    pub async fn groups(&self, after: &str, query: &str, limit: usize) -> Result<Value> {
        match self {
            Self::Local(hub) => hub.groups(after, query, limit),
            Self::Remote(path) => {
                rpc::call(
                    path,
                    &Request::Groups {
                        after: after.into(),
                        query: query.into(),
                        limit,
                    },
                    Duration::from_secs(10),
                )
                .await
            }
        }
    }
    pub async fn group(&self, group: &str) -> Result<Group> {
        match self {
            Self::Local(hub) => hub.group(group),
            Self::Remote(path) => {
                rpc::call(
                    path,
                    &Request::Group {
                        group: group.into(),
                    },
                    Duration::from_secs(10),
                )
                .await
            }
        }
    }
    pub async fn members(&self, group: &str, after: &str, limit: usize) -> Result<Value> {
        match self {
            Self::Local(hub) => hub.members(group, after, limit),
            Self::Remote(path) => {
                rpc::call(
                    path,
                    &Request::Members {
                        group: group.into(),
                        after: after.into(),
                        limit,
                    },
                    Duration::from_secs(10),
                )
                .await
            }
        }
    }
    pub async fn presence(&self, actor: &str) -> Result<ActorStatus> {
        match self {
            Self::Local(hub) => hub.presence(actor),
            Self::Remote(path) => {
                rpc::call(
                    path,
                    &Request::Presence {
                        actor: actor.into(),
                    },
                    Duration::from_secs(10),
                )
                .await
            }
        }
    }
    pub async fn people(&self, after: &str, query: &str, limit: usize) -> Result<Value> {
        match self {
            Self::Local(hub) => hub.people(after, query, limit),
            Self::Remote(path) => {
                rpc::call(
                    path,
                    &Request::People {
                        after: after.into(),
                        query: query.into(),
                        limit,
                    },
                    Duration::from_secs(10),
                )
                .await
            }
        }
    }
    pub async fn inbox(&self, token: &str, after: &str, limit: usize) -> Result<Value> {
        match self {
            Self::Local(hub) => hub.inbox(token, after, limit),
            Self::Remote(path) => {
                rpc::call(
                    path,
                    &Request::Inbox {
                        token: token.into(),
                        after: after.into(),
                        limit,
                    },
                    Duration::from_secs(10),
                )
                .await
            }
        }
    }
    pub async fn person(&self, actor: &str) -> Result<Person> {
        match self {
            Self::Local(hub) => hub.person(actor),
            Self::Remote(path) => {
                rpc::call(
                    path,
                    &Request::Person {
                        actor: actor.into(),
                    },
                    Duration::from_secs(10),
                )
                .await
            }
        }
    }
    pub async fn changes(&self) -> Result<Changes> {
        Ok(match self {
            Self::Local(hub) => Changes::Local(hub.changes()),
            Self::Remote(path) => {
                let mut stream = rpc::open(path, &Request::Changes).await?;
                ready(&mut stream).await?;
                Changes::Remote(stream)
            }
        })
    }
    pub async fn subscribe(&self, actor: &str, group: &str) -> Result<Subscription> {
        Ok(match self {
            Self::Local(hub) => Subscription::Local(hub.subscribe(actor, group)?),
            Self::Remote(path) => {
                let mut stream = rpc::open(
                    path,
                    &Request::Subscribe {
                        actor: actor.into(),
                        group: group.into(),
                    },
                )
                .await?;
                ready(&mut stream).await?;
                Subscription::Remote {
                    stream,
                    client: self.clone(),
                    actor: actor.into(),
                    group: group.into(),
                }
            }
        })
    }
}
async fn ready(stream: &mut UnixStream) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(5), rpc::read::<rpc::Reply>(stream))
        .await??
        .decode::<()>()
}
pub enum Changes {
    Local(tokio::sync::watch::Receiver<u64>),
    Remote(UnixStream),
}
impl Changes {
    pub async fn changed(&mut self) -> Result<()> {
        match self {
            Self::Local(changes) => {
                changes.changed().await?;
                Ok(())
            }
            Self::Remote(stream) => rpc::read::<rpc::Reply>(stream).await?.decode(),
        }
    }
}
pub enum Subscription {
    Local(crystal::Subscription),
    Remote {
        stream: UnixStream,
        client: Client,
        actor: String,
        group: String,
    },
}
impl Subscription {
    pub async fn receives(&self) -> Result<bool> {
        match self {
            Self::Local(sub) => sub.receives(),
            Self::Remote {
                client: Client::Remote(path),
                actor,
                group,
                ..
            } => {
                rpc::call(
                    path,
                    &Request::Receives {
                        actor: actor.clone(),
                        group: group.clone(),
                    },
                    Duration::from_secs(10),
                )
                .await
            }
            _ => unreachable!(),
        }
    }
    pub async fn recv(&mut self) -> Result<Option<Arc<Message>>> {
        match self {
            Self::Local(sub) => sub.recv().await,
            Self::Remote { stream, .. } => Ok(rpc::read::<rpc::Reply>(stream)
                .await?
                .decode::<Option<Message>>()?
                .map(Arc::new)),
        }
    }
    pub async fn wait_control(&mut self) -> Result<()> {
        match self {
            Self::Local(sub) => sub.wait_control().await,
            Self::Remote { .. } => {
                self.recv().await?;
                Ok(())
            }
        }
    }
    pub fn replayed(&self, count: usize) {
        if let Self::Local(sub) = self {
            sub.replayed(count);
        }
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
        Request::Probe => Ok(json!(info())),
        Request::Metrics => Ok(hub.metrics()),
        Request::Receives { actor, group } => {
            hub.authorize(&actor, &group)?;
            ensure!(hub.metrics()["healthy"] == true, "durable writer fault");
            Ok(json!(hub.presence(&actor)?.presence.receives()))
        }
        Request::Register { actors } => Ok(json!(hub.register(actors).await?)),
        Request::SavePerson { person } => Ok(json!(hub.save_person(person).await?)),
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
        Request::ChangeGoal { change } => Ok(json!(hub.change_goal(change).await?)),
        Request::Work { token, change } => Ok(json!(hub.work(&token, change).await?)),
        Request::WorkAs { actor, change } => Ok(json!(hub.work_as(&actor, change).await?)),
        Request::Goals {
            group,
            scenario,
            after,
            active,
        } => Ok(json!(hub.goals(&group, &scenario, &after, active).await?)),
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
async fn reply(stream: &mut UnixStream, value: Result<Value>) -> Result<()> {
    tokio::time::timeout(
        Duration::from_secs(5),
        rpc::write(stream, &rpc::Reply::from_result(value)),
    )
    .await?
}
async fn disconnected(stream: &UnixStream) {
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
async fn connection(hub: Hub, mut stream: UnixStream) -> Result<()> {
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
mod tests;
