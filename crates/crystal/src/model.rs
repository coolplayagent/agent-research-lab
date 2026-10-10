use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

pub const MAX_ACTORS: usize = 100_000;
pub const MAX_GROUPS: usize = 100_000;
pub const MAX_MEMBERSHIPS: usize = 1_000_000;
pub const MAX_MEMBERS: usize = 20_000;
pub const MAX_MESSAGES: u64 = 1_000_000;
pub const MAX_TEXT: usize = 4096;
pub const PAGE_SIZE: usize = 128;
pub const LIVE_CAPACITY: usize = 256;
pub const INGRESS_CAPACITY: usize = 4096;

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
pub fn id(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.len() <= 100
            && value
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_'),
        "identifier must be 1..100 ASCII letters, digits, underscores or hyphens"
    );
    Ok(())
}
pub fn text(value: &str, max: usize) -> Result<()> {
    ensure!(
        !value.trim().is_empty()
            && value.len() <= max
            && !value
                .chars()
                .any(|c| c.is_control() && c != '\n' && c != '\t'),
        "invalid text or text exceeds bound"
    );
    Ok(())
}
pub use contracts::Presence;

pub use contracts::GroupKind;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Person {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub application_id: Option<String>,
}
impl Person {
    pub fn validate(&self) -> Result<()> {
        id(&self.id)?;
        text(&self.name, 256)?;
        if let Some(application) = &self.application_id {
            id(application)?;
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Group {
    pub id: String,
    pub title: String,
    pub topic: String,
    pub private: bool,
    #[serde(default)]
    pub kind: GroupKind,
    pub archived: bool,
    pub pinned: bool,
    pub revision: u64,
    pub created_ms: u64,
    pub last_sequence: u64,
    pub member_count: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewGroup {
    pub id: String,
    pub title: String,
    pub topic: String,
    #[serde(default)]
    pub private: bool,
    #[serde(default)]
    pub kind: GroupKind,
    pub members: Vec<String>,
}
impl NewGroup {
    pub fn validate(&self) -> Result<()> {
        id(&self.id)?;
        text(&self.title, 256)?;
        text(&self.topic, 2048)?;
        ensure!(
            self.kind != GroupKind::Temporary || self.private,
            "temporary conversations must have a private audience"
        );
        ensure!(
            self.kind != GroupKind::Board || !self.private,
            "boards use managed group membership"
        );
        ensure!(
            !self.members.is_empty() && self.members.len() <= MAX_MEMBERS,
            "group requires 1..20000 digital people"
        );
        let mut seen = std::collections::BTreeSet::new();
        for actor in &self.members {
            id(actor)?;
            ensure!(seen.insert(actor), "duplicate group member");
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Publish {
    pub group_id: String,
    pub request_id: String,
    pub text: String,
    #[serde(default)]
    pub reply_to: Option<u64>,
}
impl Publish {
    pub fn validate(&self) -> Result<()> {
        id(&self.group_id)?;
        id(&self.request_id)?;
        ensure!(
            self.reply_to.is_none_or(|v| v > 0 && v <= i64::MAX as u64),
            "invalid reply sequence"
        );
        text(&self.text, MAX_TEXT)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event: Option<GoalEvent>,
    pub sequence: u64,
    pub group_id: String,
    pub sender_id: String,
    pub request_id: String,
    pub text: String,
    pub reply_to: Option<u64>,
    pub accepted_ms: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Receipt {
    pub message: Message,
    pub duplicate: bool,
    pub durable: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroupChange {
    pub id: String,
    pub revision: u64,
    pub title: String,
    pub topic: String,
    pub archived: bool,
    pub pinned: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ActorStatus {
    pub person_id: String,
    pub presence: Presence,
    pub connections: usize,
}

/// Host-authored audit data. Worker publish requests cannot set this field.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GoalEvent {
    pub kind: contracts::GoalEventKind,
    pub goal_id: String,
    pub title: String,
    pub person_id: Option<String>,
    pub work_id: Option<String>,
    pub attempt: Option<u32>,
}
