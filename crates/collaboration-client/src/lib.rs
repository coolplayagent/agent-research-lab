//! Durable collaboration storage client and independently hosted service.
use anyhow::Result;
use collaboration_model::*;
use rpc::Stream;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use service_protocol as rpc;
use std::{path::PathBuf, sync::Arc, time::Duration};
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    Goal {
        id: String,
    },
    Outcomes {
        token: Option<String>,
        command: Box<OutcomeCommand>,
    },
    EvolutionChange {
        change: EvolutionChange,
    },
    EvolutionRun {
        run_id: String,
    },
    EvolutionRuns {
        group: String,
        scenario: String,
        after: String,
        active: bool,
    },
    Discuss {
        input: GoalInput,
        message: Publish,
    },
    ResearchChange {
        change: ResearchChange,
    },
    Topics {
        after: String,
        query: String,
    },
    Subjects {
        topic: String,
        after: String,
    },
    Graph {
        topic: String,
        subject: String,
        after: u64,
    },
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
    ArchiveUnbound {
        retained: std::collections::BTreeSet<String>,
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
    Delete {
        target: DeleteTarget,
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
    CurrentGoal {
        group: String,
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
/// Embedded adapters are supplied by the host; clients never open a database.
pub type LocalFuture<'a, T> =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<T>> + Send + 'a>>;
pub trait EmbeddedStorage: Send + Sync {
    fn request(&self, request: Request) -> LocalFuture<'_, Value>;
    fn changes(&self) -> tokio::sync::watch::Receiver<u64>;
    fn subscribe(&self, actor: &str, group: &str) -> Result<Box<dyn LocalSubscription>>;
}
pub trait LocalSubscription: Send + Sync {
    fn receives(&self) -> Result<bool>;
    fn recv(&mut self) -> LocalFuture<'_, Option<Arc<Message>>>;
    fn wait_control(&mut self) -> LocalFuture<'_, ()>;
    fn replayed(&self, count: usize);
}
#[derive(Clone)]
pub enum Client {
    Local(Arc<dyn EmbeddedStorage>),
    Remote(PathBuf),
}
impl Client {
    async fn call<T: serde::de::DeserializeOwned>(
        &self,
        request: &Request,
        timeout: Duration,
    ) -> Result<T> {
        match self {
            Self::Local(adapter) => Ok(serde_json::from_value(
                tokio::time::timeout(timeout, adapter.request(request.clone())).await??,
            )?),
            Self::Remote(path) => rpc::call(path, request, timeout).await,
        }
    }
    pub async fn evolution_change(&self, change: EvolutionChange) -> Result<EvolutionRun> {
        self.call(
            &Request::EvolutionChange { change },
            Duration::from_secs(10),
        )
        .await
    }
    pub async fn evolution_run(&self, run_id: &str) -> Result<EvolutionRun> {
        self.call(
            &Request::EvolutionRun {
                run_id: run_id.into(),
            },
            Duration::from_secs(10),
        )
        .await
    }
    pub async fn evolution_runs(
        &self,
        group: &str,
        scenario: &str,
        after: &str,
        active: bool,
    ) -> Result<Value> {
        self.call(
            &Request::EvolutionRuns {
                group: group.into(),
                scenario: scenario.into(),
                after: after.into(),
                active,
            },
            Duration::from_secs(10),
        )
        .await
    }
    pub async fn outcomes(&self, token: Option<&str>, command: OutcomeCommand) -> Result<Value> {
        self.call(
            &Request::Outcomes {
                token: token.map(str::to_owned),
                command: Box::new(command),
            },
            Duration::from_secs(15),
        )
        .await
    }
    pub async fn research_change(&self, change: ResearchChange) -> Result<Value> {
        self.call(&Request::ResearchChange { change }, Duration::from_secs(10))
            .await
    }
    pub async fn topics(&self, after: &str, query: &str) -> Result<Value> {
        self.call(
            &Request::Topics {
                after: after.into(),
                query: query.into(),
            },
            Duration::from_secs(10),
        )
        .await
    }
    pub async fn subjects(&self, topic: &str, after: &str) -> Result<Value> {
        self.call(
            &Request::Subjects {
                topic: topic.into(),
                after: after.into(),
            },
            Duration::from_secs(10),
        )
        .await
    }
    pub async fn graph(&self, topic: &str, subject: &str, after: u64) -> Result<Value> {
        self.call(
            &Request::Graph {
                topic: topic.into(),
                subject: subject.into(),
                after,
            },
            Duration::from_secs(10),
        )
        .await
    }
    pub fn remote(path: PathBuf) -> Result<Self> {
        rpc::endpoint(&path)?;
        Ok(Self::Remote(path))
    }
    pub async fn probe(&self) -> Result<rpc::ServiceInfo> {
        self.call(&Request::Probe, Duration::from_secs(5)).await
    }
    pub async fn metrics(&self) -> Result<Value> {
        self.call(&Request::Metrics, Duration::from_secs(10)).await
    }
    pub async fn register(&self, actors: Vec<String>) -> Result<()> {
        self.call(&Request::Register { actors }, Duration::from_secs(10))
            .await
    }
    pub async fn save_person(&self, person: Person) -> Result<()> {
        self.call(&Request::SavePerson { person }, Duration::from_secs(10))
            .await
    }
    pub async fn archive_unbound(
        &self,
        retained: std::collections::BTreeSet<String>,
    ) -> Result<usize> {
        self.call(
            &Request::ArchiveUnbound { retained },
            Duration::from_secs(15),
        )
        .await
    }
    pub async fn register_operator(&self) -> Result<()> {
        self.call(&Request::RegisterOperator {}, Duration::from_secs(10))
            .await
    }
    pub async fn convert(&self, group: &str, revision: u64, title: String) -> Result<Group> {
        self.call(
            &Request::Convert {
                group: group.into(),
                revision,
                title,
            },
            Duration::from_secs(10),
        )
        .await
    }
    pub async fn fork(
        &self,
        source: &str,
        group: NewGroup,
        history_from: Option<u64>,
    ) -> Result<Group> {
        self.call(
            &Request::Fork {
                source: source.into(),
                group,
                history_from,
            },
            Duration::from_secs(10),
        )
        .await
    }
    pub async fn create_group(&self, group: NewGroup) -> Result<Group> {
        self.call(&Request::CreateGroup { group }, Duration::from_secs(10))
            .await
    }
    pub async fn delete(&self, target: DeleteTarget) -> Result<DeletionResult> {
        self.call(&Request::Delete { target }, Duration::from_secs(10))
            .await
    }
    pub async fn update_group(&self, change: GroupChange) -> Result<()> {
        self.call(&Request::UpdateGroup { change }, Duration::from_secs(10))
            .await
    }
    pub async fn membership(&self, group: &str, actor: &str, add: bool) -> Result<()> {
        self.call(
            &Request::Membership {
                group: group.into(),
                actor: actor.into(),
                add,
            },
            Duration::from_secs(10),
        )
        .await
    }
    pub async fn set_presence(&self, actor: &str, state: Presence) -> Result<()> {
        self.call(
            &Request::SetPresence {
                actor: actor.into(),
                state,
            },
            Duration::from_secs(10),
        )
        .await
    }
    pub async fn grant(&self, actor: &str, lifetime_seconds: u64) -> Result<String> {
        self.call(
            &Request::Grant {
                actor: actor.into(),
                lifetime_seconds,
            },
            Duration::from_secs(10),
        )
        .await
    }
    pub async fn publish(&self, token: &str, input: Publish) -> Result<Receipt> {
        self.call(
            &Request::Publish {
                token: token.into(),
                input,
            },
            Duration::from_secs(10),
        )
        .await
    }
    pub async fn publish_as(&self, actor: &str, input: Publish) -> Result<Receipt> {
        self.call(
            &Request::PublishAs {
                actor: actor.into(),
                input,
            },
            Duration::from_secs(10),
        )
        .await
    }
    pub async fn history(&self, group: &str, after: u64, limit: usize) -> Result<Vec<Message>> {
        self.call(
            &Request::History {
                group: group.into(),
                after,
                limit,
            },
            Duration::from_secs(10),
        )
        .await
    }
    pub async fn history_before(
        &self,
        group: &str,
        before: u64,
        limit: usize,
    ) -> Result<Vec<Message>> {
        self.call(
            &Request::HistoryBefore {
                group: group.into(),
                before,
                limit,
            },
            Duration::from_secs(10),
        )
        .await
    }
    pub async fn acknowledge(&self, token: &str, group: &str, sequence: u64) -> Result<()> {
        self.call(
            &Request::Acknowledge {
                token: token.into(),
                group: group.into(),
                sequence,
            },
            Duration::from_secs(10),
        )
        .await
    }
    pub async fn cursor(&self, actor: &str, group: &str) -> Result<u64> {
        self.call(
            &Request::Cursor {
                actor: actor.into(),
                group: group.into(),
            },
            Duration::from_secs(10),
        )
        .await
    }
    pub async fn discuss(&self, input: GoalInput, message: Publish) -> Result<Value> {
        self.call(
            &Request::Discuss { input, message },
            Duration::from_secs(10),
        )
        .await
    }
    pub async fn change_goal(&self, change: GoalChange) -> Result<Goal> {
        self.call(&Request::ChangeGoal { change }, Duration::from_secs(10))
            .await
    }
    pub async fn work(&self, token: &str, change: WorkChange) -> Result<Goal> {
        self.call(
            &Request::Work {
                token: token.into(),
                change,
            },
            Duration::from_secs(10),
        )
        .await
    }
    pub async fn work_as(&self, actor: &str, change: WorkChange) -> Result<Goal> {
        self.call(
            &Request::WorkAs {
                actor: actor.into(),
                change,
            },
            Duration::from_secs(10),
        )
        .await
    }
    pub async fn goal(&self, id: &str) -> Result<Goal> {
        self.call(&Request::Goal { id: id.into() }, Duration::from_secs(10))
            .await
    }
    pub async fn goals(
        &self,
        group: &str,
        scenario: &str,
        after: &str,
        active: bool,
    ) -> Result<Value> {
        self.call(
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
    pub async fn current_goal(&self, group: &str) -> Result<Value> {
        self.call(
            &Request::CurrentGoal {
                group: group.into(),
            },
            Duration::from_secs(10),
        )
        .await
    }
    pub async fn assigned_goals(&self, token: &str, after: &str) -> Result<Value> {
        self.call(
            &Request::AssignedGoals {
                token: token.into(),
                after: after.into(),
            },
            Duration::from_secs(10),
        )
        .await
    }
    pub async fn authenticate(&self, token: &str) -> Result<String> {
        self.call(
            &Request::Authenticate {
                token: token.into(),
            },
            Duration::from_secs(10),
        )
        .await
    }
    pub async fn grant_expiry(&self, token: &str) -> Result<u64> {
        self.call(
            &Request::GrantExpiry {
                token: token.into(),
            },
            Duration::from_secs(10),
        )
        .await
    }
    pub async fn authorize(&self, actor: &str, group: &str) -> Result<()> {
        self.call(
            &Request::Authorize {
                actor: actor.into(),
                group: group.into(),
            },
            Duration::from_secs(10),
        )
        .await
    }
    pub async fn groups(&self, after: &str, query: &str, limit: usize) -> Result<Value> {
        self.call(
            &Request::Groups {
                after: after.into(),
                query: query.into(),
                limit,
            },
            Duration::from_secs(10),
        )
        .await
    }
    pub async fn group(&self, group: &str) -> Result<Group> {
        self.call(
            &Request::Group {
                group: group.into(),
            },
            Duration::from_secs(10),
        )
        .await
    }
    pub async fn members(&self, group: &str, after: &str, limit: usize) -> Result<Value> {
        self.call(
            &Request::Members {
                group: group.into(),
                after: after.into(),
                limit,
            },
            Duration::from_secs(10),
        )
        .await
    }
    pub async fn presence(&self, actor: &str) -> Result<ActorStatus> {
        self.call(
            &Request::Presence {
                actor: actor.into(),
            },
            Duration::from_secs(10),
        )
        .await
    }
    pub async fn people(&self, after: &str, query: &str, limit: usize) -> Result<Value> {
        self.call(
            &Request::People {
                after: after.into(),
                query: query.into(),
                limit,
            },
            Duration::from_secs(10),
        )
        .await
    }
    pub async fn inbox(&self, token: &str, after: &str, limit: usize) -> Result<Value> {
        self.call(
            &Request::Inbox {
                token: token.into(),
                after: after.into(),
                limit,
            },
            Duration::from_secs(10),
        )
        .await
    }
    pub async fn person(&self, actor: &str) -> Result<Person> {
        self.call(
            &Request::Person {
                actor: actor.into(),
            },
            Duration::from_secs(10),
        )
        .await
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
async fn ready(stream: &mut Stream) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(5), rpc::read::<rpc::Reply>(stream))
        .await??
        .decode::<()>()
}
pub enum Changes {
    Local(tokio::sync::watch::Receiver<u64>),
    Remote(Stream),
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
    Local(Box<dyn LocalSubscription>),
    Remote {
        stream: Stream,
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
