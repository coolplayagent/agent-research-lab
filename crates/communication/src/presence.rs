//! Host-owned IM availability and bounded delivery queues. Delivery is not a read ACK.
use super::*;
use std::io::{Seek, SeekFrom};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Presence {
    Offline,
    Online,
    Chatting,
    Busy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Inbox {
    state: Presence,
    since: u64,
    delivered: BTreeSet<usize>,
    activity_offset: u64,
    partial_line: bool,
    tools: BTreeSet<String>,
    chatting_until: u64,
}
impl Inbox {
    pub(super) fn new(now: u64) -> Self {
        Self {
            state: Presence::Online,
            since: now,
            delivered: BTreeSet::new(),
            activity_offset: 0,
            partial_line: false,
            tools: BTreeSet::new(),
            chatting_until: 0,
        }
    }
    fn transition(&mut self, next: Presence, now: u64) -> Result<()> {
        ensure!(
            self.state != Presence::Offline || matches!(next, Presence::Offline | Presence::Online),
            "offline agents must reconnect before becoming busy or chatting"
        );
        if self.state != next {
            self.state = next;
            self.since = now;
        }
        if next == Presence::Chatting {
            self.chatting_until = now.saturating_add(5);
        }
        Ok(())
    }
}

pub(super) fn state(member: &MemberRecord) -> Presence {
    if member.status != OriginState::Active {
        Presence::Offline
    } else {
        member.inbox.as_ref().map_or(Presence::Online, |i| i.state)
    }
}

pub(super) fn delivered(c: &Cohort, index: usize, member: &HostMember) -> bool {
    c.messages[index].task_id == member.task_id
        || c.members
            .iter()
            .find(|m| m.identity == *member)
            .is_some_and(|m| {
                m.inbox
                    .as_ref()
                    .is_none_or(|i| i.delivered.contains(&index))
            })
}

pub(super) fn initialize(c: &mut Cohort) {
    for member in &mut c.members {
        if member.inbox.is_none() {
            // Legacy exports were immediately visible. Preserve their delivery history.
            let mut inbox = Inbox::new(member.registered_at);
            inbox.state = if member.status == OriginState::Active {
                Presence::Online
            } else {
                Presence::Offline
            };
            inbox.delivered = c
                .messages
                .iter()
                .enumerate()
                .filter(|(_, m)| visible_to(m, Some(&member.identity.task_id)))
                .map(|(n, _)| n)
                .collect();
            member.inbox = Some(inbox);
        }
    }
}

pub(super) fn validate(c: &Cohort) -> Result<()> {
    for member in &c.members {
        if let Some(inbox) = &member.inbox {
            ensure!(
                inbox.since <= c.last_time
                    && inbox.tools.len() <= 32
                    && inbox
                        .tools
                        .iter()
                        .all(|id| !id.is_empty() && id.len() <= 128)
                    && (member.status == OriginState::Active || inbox.state == Presence::Offline)
                    && inbox.delivered.iter().all(|n| c
                        .messages
                        .get(*n)
                        .is_some_and(|m| visible_to(m, Some(&member.identity.task_id)))),
                "invalid presence or delivery authority"
            );
        }
    }
    Ok(())
}

pub(super) fn flush(c: &mut Cohort, now: u64) {
    initialize(c);
    let revoked: BTreeSet<_> = c
        .members
        .iter()
        .filter(|m| matches!(m.status, OriginState::Revoked { .. }))
        .map(|m| m.identity.run_id.clone())
        .collect();
    for member in &mut c.members {
        if !matches!(state(member), Presence::Online | Presence::Chatting) {
            continue;
        }
        let inbox = member.inbox.as_mut().unwrap();
        for (n, message) in c.messages.iter().enumerate() {
            if message.expires_at > now
                && !revoked.contains(&message.run_id)
                && visible_to(message, Some(&member.identity.task_id))
            {
                inbox.delivered.insert(n);
            }
        }
    }
}

pub(super) fn set(member: &mut MemberRecord, next: Presence, now: u64) -> Result<()> {
    ensure!(
        member.status == OriginState::Active || next == Presence::Offline,
        "terminal runs cannot reconnect; register a new host-authorized run"
    );
    member
        .inbox
        .as_mut()
        .context("presence not initialized")?
        .transition(next, now)
}

pub(super) fn speak(member: &mut MemberRecord, now: u64) {
    if matches!(state(member), Presence::Online | Presence::Chatting) {
        let _ = set(member, Presence::Chatting, now);
    }
}

pub(super) fn projection(c: &Cohort, member: &MemberRecord, now: u64) -> serde_json::Value {
    let pending = c
        .messages
        .iter()
        .enumerate()
        .filter(|(n, m)| {
            m.expires_at > now
                && visible_to(m, Some(&member.identity.task_id))
                && !delivered(c, *n, &member.identity)
                && !c.members.iter().any(|origin| {
                    origin.identity.run_id == m.run_id
                        && matches!(origin.status, OriginState::Revoked { .. })
                })
        })
        .count();
    serde_json::json!({"state":state(member),"since":member.inbox.as_ref().map_or(member.registered_at,|i|i.since),"pending":pending,"source":"host_lifecycle_and_public_session_events"})
}

pub(super) fn dispatch(c: &Cohort, index: usize, now: u64) -> Vec<serde_json::Value> {
    let message = &c.messages[index];
    let recipients: BTreeSet<_> = c
        .members
        .iter()
        .filter(|m| {
            m.identity.task_id != message.task_id && visible_to(message, Some(&m.identity.task_id))
        })
        .map(|m| &m.identity.task_id)
        .collect();
    recipients
        .into_iter()
        .map(|id| {
            let status = if c
                .members
                .iter()
                .any(|m| &m.identity.task_id == id && delivered(c, index, &m.identity))
            {
                "delivered"
            } else if message.expires_at <= now {
                "expired"
            } else {
                "pending"
            };
            serde_json::json!({"task_id":id,"state":status})
        })
        .collect()
}

/// Read at most 64 KiB per live run per host poll. Ignore text and tool output;
/// only public event types and item IDs can affect availability.
pub(super) fn observe(state_dir: &Path, member: &mut MemberRecord, now: u64) -> Result<()> {
    ensure!(
        member.status == OriginState::Active,
        "cannot observe a terminal run"
    );
    if state(member) == Presence::Offline {
        set(member, Presence::Online, now)?;
    }
    let inbox = member.inbox.as_mut().context("presence not initialized")?;
    let path = state_dir
        .join("runs")
        .join(&member.identity.run_id)
        .join("process");
    let read = || -> Result<Option<File>> {
        let dir = directory(&path, false)?;
        let name = CString::new("stdout.jsonl")?;
        let fd = unsafe {
            libc::openat(
                dir.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let file = unsafe { File::from_raw_fd(fd) };
        let meta = file.metadata()?;
        ensure!(
            meta.is_file() && meta.nlink() == 1,
            "activity log must be a regular non-linked file"
        );
        Ok(Some(file))
    };
    let file = match read() {
        Ok(file) => file,
        Err(error)
            if error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) =>
        {
            None
        }
        Err(error) => return Err(error),
    };
    if let Some(mut file) = file {
        if file.metadata()?.len() < inbox.activity_offset {
            inbox.activity_offset = 0;
            inbox.partial_line = false;
            inbox.tools.clear();
        }
        file.seek(SeekFrom::Start(inbox.activity_offset))?;
        let mut bytes = vec![];
        (&mut file).take(64 * 1024).read_to_end(&mut bytes)?;
        let mut consumed = 0;
        for line in bytes.split_inclusive(|b| *b == b'\n') {
            if !line.ends_with(b"\n") {
                break;
            }
            consumed += line.len();
            if inbox.partial_line {
                inbox.partial_line = false;
                continue;
            }
            let Ok(event) = serde_json::from_slice::<serde_json::Value>(line) else {
                continue;
            };
            let kind = event["type"].as_str().unwrap_or("");
            let item = &event["item"];
            let id = item["id"]
                .as_str()
                .filter(|s| !s.is_empty() && s.len() <= 128);
            match (kind, item["type"].as_str().unwrap_or("")) {
                (
                    "item.started",
                    "command_execution" | "mcp_tool_call" | "web_search" | "file_change",
                ) => {
                    if let Some(id) = id {
                        ensure!(
                            inbox.tools.contains(id) || inbox.tools.len() < 32,
                            "too many concurrent activity items"
                        );
                        inbox.tools.insert(id.into());
                    }
                }
                (
                    "item.completed" | "item.failed",
                    "command_execution" | "mcp_tool_call" | "web_search" | "file_change",
                ) => {
                    if let Some(id) = id {
                        inbox.tools.remove(id);
                    }
                }
                ("item.started" | "item.completed", "agent_message") => {
                    inbox.chatting_until = now.saturating_add(5)
                }
                ("turn.completed" | "turn.failed", _) => inbox.tools.clear(),
                _ => (),
            }
        }
        if consumed == 0 && bytes.len() == 64 * 1024 {
            consumed = bytes.len();
            inbox.partial_line = true;
        }
        inbox.activity_offset += consumed as u64;
    }
    let next = if !inbox.tools.is_empty() {
        Presence::Busy
    } else if inbox.chatting_until > now {
        Presence::Chatting
    } else {
        Presence::Online
    };
    // Do not extend a completed chat's cooldown on each polling tick.
    if inbox.state != next {
        inbox.state = next;
        inbox.since = now;
    }
    Ok(())
}
