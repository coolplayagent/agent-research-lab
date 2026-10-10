//! Durable digital-person conversations, independent of model execution.
//! Host-authorized membership, ordered logs, bounded live delivery and replay.
mod conversations;
mod directory;
mod goal_model;
mod goal_work;
mod goals;
mod groups;
mod metrics;
mod model;
mod schema;
mod store;
mod writer;
use anyhow::{Context, Result, anyhow, ensure};
pub use goal_model::*;
use metrics::Metrics;
pub use model::*;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashMap},
    io::Read,
    path::Path,
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread::JoinHandle,
    time::Instant,
};
use store::{Catalog, Store};
use tokio::sync::{broadcast, mpsc, oneshot, watch};

pub(crate) enum Control {
    Goal(goals::GoalCommand),
    #[cfg(test)]
    ReadOnlyFault,
    Register(Vec<String>),
    Person(Person),
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
    Create(NewGroup),
    Update(GroupChange),
    Membership {
        group: String,
        actor: String,
        add: bool,
    },
    Presence {
        actor: String,
        state: Presence,
    },
    Grant {
        actor: String,
        hash: String,
        expires: u64,
    },
    HistoryBefore {
        group: String,
        before: u64,
        limit: usize,
    },
    History {
        group: String,
        after: u64,
        limit: usize,
    },
    Ack {
        actor: String,
        group: String,
        sequence: u64,
    },
    Cursor {
        actor: String,
        group: String,
    },
}
pub(crate) enum Command {
    Publish {
        actor: String,
        credential: Option<String>,
        input: Publish,
        started: Instant,
        reply: oneshot::Sender<Result<Receipt>>,
    },
    Control {
        op: Control,
        reply: oneshot::Sender<Result<Value>>,
    },
}
pub(crate) struct Shared {
    catalog: RwLock<Catalog>,
    feeds: Mutex<HashMap<String, broadcast::Sender<Arc<Message>>>>,
    connections: Mutex<BTreeMap<String, usize>>,
    changed: watch::Sender<u64>,
    controls: Mutex<HashMap<String, watch::Sender<u64>>>,
    failed: AtomicBool,
    catalog_revision: AtomicU64,
    metrics: Metrics,
}
impl Shared {
    fn signal(&self) {
        self.changed.send_modify(|v| *v = v.saturating_add(1));
    }
    fn fail(&self) {
        self.failed.store(true, Ordering::Release);
        for control in self.controls.lock().unwrap().values() {
            control.send_modify(|v| *v = v.wrapping_add(1));
        }
        self.signal();
    }
    fn directory_signal(&self) {
        self.catalog_revision.fetch_add(1, Ordering::Relaxed);
        self.signal();
    }
    fn actor_signal(&self, actor: &str) {
        if let Some(signal) = self.controls.lock().unwrap().get(actor) {
            signal.send_modify(|v| *v = v.wrapping_add(1));
        }
    }
    fn authorized(&self, actor: &str, group: &str) -> Result<()> {
        ensure!(
            self.catalog
                .read()
                .unwrap()
                .members
                .get(group)
                .is_some_and(|m| m.contains(actor)),
            "digital person is not a member of this conversation"
        );
        Ok(())
    }
}
struct Worker {
    thread: Mutex<Option<JoinHandle<()>>>,
}
impl Drop for Worker {
    fn drop(&mut self) {
        if let Some(thread) = self.thread.get_mut().unwrap().take() {
            let _ = thread.join();
        }
    }
}
#[derive(Clone)]
pub struct Hub {
    // Senders must drop before the final Worker joins its receiver thread.
    tx: mpsc::Sender<Command>,
    shared: Arc<Shared>,
    _worker: Arc<Worker>,
}
impl Hub {
    pub fn open(state: &Path) -> Result<Self> {
        std::fs::create_dir_all(state)?;
        let lock = storage::lock(&state.join("writer.lock"))?;
        let store = Store::open(&state.join("messages.sqlite"))?;
        let catalog = store.catalog()?;
        let (changed, _) = watch::channel(0);
        let shared = Arc::new(Shared {
            catalog: RwLock::new(catalog),
            feeds: Mutex::new(HashMap::new()),
            connections: Mutex::new(BTreeMap::new()),
            changed,
            metrics: Metrics::default(),
            controls: Mutex::new(HashMap::new()),
            failed: AtomicBool::new(false),
            catalog_revision: AtomicU64::new(0),
        });
        let (tx, rx) = mpsc::channel(INGRESS_CAPACITY);
        let worker_shared = shared.clone();
        let thread = std::thread::Builder::new()
            .name("crystal-writer".into())
            .spawn(move || {
                let _lock = lock;
                writer::run(store, rx, worker_shared)
            })?;
        Ok(Self {
            tx,
            shared,
            _worker: Arc::new(Worker {
                thread: Mutex::new(Some(thread)),
            }),
        })
    }
    fn submit(&self, cmd: Command) -> Result<()> {
        ensure!(
            !self.shared.failed.load(Ordering::Acquire),
            "durable writer fault; restart after storage recovery"
        );
        self.shared.metrics.queued.fetch_add(1, Ordering::Relaxed);
        if let Err(error) = self.tx.try_send(cmd) {
            self.shared.metrics.queued.fetch_sub(1, Ordering::Relaxed);
            self.shared.metrics.rejected.fetch_add(1, Ordering::Relaxed);
            return Err(anyhow!(
                "communication ingress unavailable or full: {}",
                if matches!(error, mpsc::error::TrySendError::Full(_)) {
                    "backpressure"
                } else {
                    "closed"
                }
            ));
        }
        Ok(())
    }
    async fn control(&self, op: Control) -> Result<Value> {
        let (reply, receive) = oneshot::channel();
        self.submit(Command::Control { op, reply })?;
        receive.await.context("communication writer stopped")?
    }
    /// Host registration mirrors stable identity IDs only; Soul/memory stay with people.
    pub async fn register(&self, actors: Vec<String>) -> Result<()> {
        self.control(Control::Register(actors)).await?;
        Ok(())
    }
    pub async fn save_person(&self, person: Person) -> Result<()> {
        self.control(Control::Person(person)).await?;
        Ok(())
    }
    pub async fn convert(&self, group: &str, revision: u64, title: String) -> Result<Group> {
        Ok(serde_json::from_value(
            self.control(Control::Convert {
                group: group.into(),
                revision,
                title,
            })
            .await?,
        )?)
    }
    pub async fn fork(
        &self,
        source: &str,
        group: NewGroup,
        history_from: Option<u64>,
    ) -> Result<Group> {
        Ok(serde_json::from_value(
            self.control(Control::Fork {
                source: source.into(),
                group,
                history_from,
            })
            .await?,
        )?)
    }
    pub async fn create_group(&self, group: NewGroup) -> Result<Group> {
        Ok(serde_json::from_value(
            self.control(Control::Create(group)).await?,
        )?)
    }
    pub async fn update_group(&self, change: GroupChange) -> Result<()> {
        self.control(Control::Update(change)).await?;
        Ok(())
    }
    pub async fn membership(&self, group: &str, actor: &str, add: bool) -> Result<()> {
        self.control(Control::Membership {
            group: group.into(),
            actor: actor.into(),
            add,
        })
        .await?;
        Ok(())
    }
    pub async fn set_presence(&self, actor: &str, state: Presence) -> Result<()> {
        self.control(Control::Presence {
            actor: actor.into(),
            state,
        })
        .await?;
        Ok(())
    }
    /// Rotation revokes the old grant. The plaintext is returned once, never persisted.
    pub async fn grant(&self, actor: &str, lifetime_seconds: u64) -> Result<String> {
        ensure!(
            (1..=86400).contains(&lifetime_seconds),
            "grant lifetime must be 1..86400 seconds"
        );
        let mut random = [0u8; 32];
        std::fs::File::open("/dev/urandom")?.read_exact(&mut random)?;
        let token = storage::digest(&random);
        self.control(Control::Grant {
            actor: actor.into(),
            hash: storage::digest(token.as_bytes()),
            expires: now_ms() + lifetime_seconds * 1000,
        })
        .await?;
        Ok(token)
    }
    pub fn authenticate(&self, token: &str) -> Result<String> {
        ensure!(token.len() == 64, "invalid communication credential");
        let catalog = self.shared.catalog.read().unwrap();
        let (actor, expires) = catalog
            .grants
            .get(&storage::digest(token.as_bytes()))
            .context("communication credential revoked or unknown")?;
        ensure!(*expires > now_ms(), "communication credential expired");
        Ok(actor.clone())
    }
    pub fn grant_expiry(&self, token: &str) -> Result<u64> {
        self.authenticate(token)?;
        Ok(self.shared.catalog.read().unwrap().grants[&storage::digest(token.as_bytes())].1)
    }
    pub fn authorize(&self, actor: &str, group: &str) -> Result<()> {
        self.shared.authorized(actor, group)
    }
    pub async fn publish(&self, token: &str, input: Publish) -> Result<Receipt> {
        let actor = self.authenticate(token)?;
        self.enqueue(&actor, Some(storage::digest(token.as_bytes())), input)
            .await
    }
    /// Host adapter boundary. Agent HTTP callers must authenticate through `publish`.
    pub async fn publish_as(&self, actor: &str, input: Publish) -> Result<Receipt> {
        self.enqueue(actor, None, input).await
    }
    async fn enqueue(
        &self,
        actor: &str,
        credential: Option<String>,
        input: Publish,
    ) -> Result<Receipt> {
        input.validate()?;
        self.shared.authorized(actor, &input.group_id)?;
        let (reply, receive) = oneshot::channel();
        self.submit(Command::Publish {
            actor: actor.into(),
            credential,
            input,
            started: Instant::now(),
            reply,
        })?;
        receive.await.context("communication writer stopped")?
    }
    pub async fn history(&self, group: &str, after: u64, limit: usize) -> Result<Vec<Message>> {
        id(group)?;
        ensure!(
            self.shared
                .catalog
                .read()
                .unwrap()
                .groups
                .contains_key(group),
            "unknown conversation"
        );
        Ok(serde_json::from_value(
            self.control(Control::History {
                group: group.into(),
                after,
                limit,
            })
            .await?,
        )?)
    }
    pub async fn history_before(
        &self,
        group: &str,
        before: u64,
        limit: usize,
    ) -> Result<Vec<Message>> {
        id(group)?;
        Ok(serde_json::from_value(
            self.control(Control::HistoryBefore {
                group: group.into(),
                before,
                limit,
            })
            .await?,
        )?)
    }
    pub async fn acknowledge(&self, token: &str, group: &str, sequence: u64) -> Result<()> {
        let actor = self.authenticate(token)?;
        self.shared.authorized(&actor, group)?;
        self.control(Control::Ack {
            actor,
            group: group.into(),
            sequence,
        })
        .await?;
        Ok(())
    }
    pub async fn cursor(&self, actor: &str, group: &str) -> Result<u64> {
        Ok(serde_json::from_value(
            self.control(Control::Cursor {
                actor: actor.into(),
                group: group.into(),
            })
            .await?,
        )?)
    }
    pub fn groups(&self, after: &str, query: &str, limit: usize) -> Result<Value> {
        ensure!(limit > 0 && limit <= 100, "group page exceeds bound");
        ensure!(query.len() <= 256, "group search exceeds bound");
        let catalog = self.shared.catalog.read().unwrap();
        let query = query.to_lowercase();
        let cursor = if after.is_empty() {
            None
        } else {
            let (kind, id) = after.split_once(':').context("invalid group cursor")?;
            ensure!(kind == "0" || kind == "1", "invalid group cursor");
            Some((kind == "1", id))
        };
        let mut rows: Vec<_> = catalog
            .groups
            .values()
            .filter(|g| {
                cursor.is_none_or(|c| (!g.pinned, g.id.as_str()) > c)
                    && (g.title.to_lowercase().contains(&query)
                        || g.topic.to_lowercase().contains(&query))
            })
            .collect();
        rows.sort_by(|a, b| (!a.pinned, &a.id).cmp(&(!b.pinned, &b.id)));
        let next = (rows.len() > limit).then(|| {
            format!(
                "{}:{}",
                u8::from(!rows[limit - 1].pinned),
                rows[limit - 1].id
            )
        });
        Ok(
            json!({"groups":rows.into_iter().take(limit).collect::<Vec<_>>(),"next_after":next,"total":catalog.groups.len()}),
        )
    }

    pub fn group(&self, group: &str) -> Result<Group> {
        self.shared
            .catalog
            .read()
            .unwrap()
            .groups
            .get(group)
            .cloned()
            .context("unknown conversation")
    }
    pub fn members(&self, group: &str, after: &str, limit: usize) -> Result<Value> {
        ensure!(limit > 0 && limit <= 100, "member page exceeds bound");
        let catalog = self.shared.catalog.read().unwrap();
        let members = catalog.members.get(group).context("unknown conversation")?;
        let rows: Vec<_> = members
            .iter()
            .filter(|id| id.as_str() > after)
            .take(limit + 1)
            .cloned()
            .collect();
        let next = (rows.len() > limit).then(|| rows[limit - 1].clone());
        drop(catalog);
        Ok(
            json!({"members":rows.into_iter().take(limit).map(|id|self.presence(&id)).collect::<Result<Vec<_>>>()?,"next_after":next}),
        )
    }
    pub fn presence(&self, actor: &str) -> Result<ActorStatus> {
        let desired = *self
            .shared
            .catalog
            .read()
            .unwrap()
            .actors
            .get(actor)
            .context("unknown digital person")?;
        let connections = self
            .shared
            .connections
            .lock()
            .unwrap()
            .get(actor)
            .copied()
            .unwrap_or(0);
        Ok(ActorStatus {
            person_id: actor.into(),
            presence: if connections == 0 {
                Presence::Offline
            } else {
                desired
            },
            connections,
        })
    }
    pub fn changes(&self) -> watch::Receiver<u64> {
        self.shared.changed.subscribe()
    }
    pub fn metrics(&self) -> Value {
        let catalog = self.shared.catalog.read().unwrap();
        let mut value = self.shared.metrics.view();
        value["catalog_revision"] = json!(self.shared.catalog_revision.load(Ordering::Relaxed));
        value["healthy"] = json!(!self.shared.failed.load(Ordering::Acquire));
        value["actors"] = json!(catalog.actors.len());
        value["groups"] = json!(catalog.groups.len());
        value["stored_messages"] = json!(catalog.messages);
        value["limits"] = json!({"actors":MAX_ACTORS,"groups":MAX_GROUPS,"members_per_group":MAX_MEMBERS,"messages":MAX_MESSAGES,"ingress":INGRESS_CAPACITY,"live_ring":LIVE_CAPACITY});
        value["durability"] =
            json!("SQLite WAL synchronous=FULL; acknowledgement follows transaction commit");
        value["topology"] = json!("single_host");
        value
    }
    /// Subscribe before replay to close the history/live race. Sequence IDs deduplicate overlap.
    pub fn subscribe(&self, actor: &str, group: &str) -> Result<Subscription> {
        ensure!(
            !self.shared.failed.load(Ordering::Acquire),
            "durable writer fault"
        );
        self.shared.authorized(actor, group)?;
        let receiver = self
            .shared
            .feeds
            .lock()
            .unwrap()
            .entry(group.into())
            .or_insert_with(|| broadcast::channel(LIVE_CAPACITY).0)
            .subscribe();
        *self
            .shared
            .connections
            .lock()
            .unwrap()
            .entry(actor.into())
            .or_default() += 1;
        let live = self
            .shared
            .metrics
            .subscribers
            .fetch_add(1, Ordering::Relaxed)
            + 1;
        self.shared
            .metrics
            .peak_subscribers
            .fetch_max(live, Ordering::Relaxed);
        self.shared.directory_signal();
        let control = self
            .shared
            .controls
            .lock()
            .unwrap()
            .entry(actor.into())
            .or_insert_with(|| watch::channel(0).0)
            .subscribe();
        Ok(Subscription {
            control,
            actor: actor.into(),
            group: group.into(),
            receiver,
            shared: self.shared.clone(),
        })
    }
}
pub struct Subscription {
    actor: String,
    group: String,
    receiver: broadcast::Receiver<Arc<Message>>,
    control: watch::Receiver<u64>,
    shared: Arc<Shared>,
}
impl Subscription {
    pub fn receives(&self) -> Result<bool> {
        ensure!(
            !self.shared.failed.load(Ordering::Acquire),
            "durable writer fault"
        );
        self.shared.authorized(&self.actor, &self.group)?;
        Ok(self.shared.catalog.read().unwrap().actors[&self.actor].receives())
    }
    pub async fn wait_control(&mut self) -> Result<()> {
        self.control.changed().await?;
        Ok(())
    }
    pub async fn recv(&mut self) -> Result<Option<Arc<Message>>> {
        let received = tokio::select! {
            result = self.receiver.recv() => result,
            _ = self.control.changed() => return Ok(None),
        };
        match received {
            Ok(message) => Ok(Some(message)),
            Err(broadcast::error::RecvError::Lagged(_)) => {
                self.shared.metrics.lagged.fetch_add(1, Ordering::Relaxed);
                Ok(None)
            }
            Err(error) => Err(error.into()),
        }
    }
    pub fn replayed(&self, count: usize) {
        self.shared
            .metrics
            .replayed
            .fetch_add(count as u64, Ordering::Relaxed);
    }
}
impl Drop for Subscription {
    fn drop(&mut self) {
        let mut connections = self.shared.connections.lock().unwrap();
        if let Some(count) = connections.get_mut(&self.actor) {
            *count -= 1;
            if *count == 0 {
                connections.remove(&self.actor);
            }
        }
        drop(connections);
        let mut feeds = self.shared.feeds.lock().unwrap();
        // The dropping receiver still contributes one to receiver_count.
        if feeds
            .get(&self.group)
            .is_some_and(|feed| feed.receiver_count() == 1)
        {
            feeds.remove(&self.group);
        }
        drop(feeds);
        self.shared
            .metrics
            .subscribers
            .fetch_sub(1, Ordering::Relaxed);
        self.shared.directory_signal();
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod im_tests;

#[cfg(test)]
mod goal_tests;
