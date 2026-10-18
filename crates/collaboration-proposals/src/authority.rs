//! Host-owned cohort records and complete retained-authority validation.
use crate::{
    audience::reply_audience,
    limits::{
        MAX_COHORT_MESSAGES, MAX_MEMBERS, MAX_REPLY_DEPTH, MAX_RUN_MESSAGES, MAX_TASK_MESSAGES,
        OUTBOX_SLOTS, TTL_SECONDS,
    },
    presence,
    protocol::{HostMember, Message, digest_id},
};
use anyhow::{Context, Result, ensure};
use contracts::OriginState;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MemberRecord {
    pub(super) identity: HostMember,
    pub(super) status: OriginState,
    pub(super) registered_at: u64,
    pub(super) accepted: usize,
    pub(super) slots: BTreeMap<usize, Seen>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) inbox: Option<presence::Inbox>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Seen {
    pub(super) digest: Option<String>,
    pub(super) outcome: String,
    pub(super) changed_after_consumption: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Cohort {
    pub(super) schema_version: u32,
    pub(super) id: String,
    pub(super) revision: u64,
    pub(super) created_at: u64,
    pub(super) last_time: u64,
    pub(super) closed_at: Option<u64>,
    pub(super) cursor: usize,
    pub(super) members: Vec<MemberRecord>,
    pub(super) messages: Vec<Message>,
}

pub(super) fn validate(c: &Cohort, id: &str) -> Result<()> {
    ensure!(
        c.schema_version == 1
            && c.id == id
            && c.members.len() <= MAX_MEMBERS
            && c.messages.len() <= MAX_COHORT_MESSAGES,
        "invalid cohort authority"
    );
    let mut runs = BTreeSet::new();
    for m in &c.members {
        m.identity.validate()?;
        ensure!(
            m.identity.cohort_id == id
                && runs.insert(&m.identity.run_id)
                && m.accepted <= MAX_RUN_MESSAGES
                && m.slots.len() <= OUTBOX_SLOTS
                && m.slots.keys().all(|s| *s < OUTBOX_SLOTS),
            "invalid member authority"
        );
        match &m.status {
            OriginState::Completed { receipt_sha256 } => digest_id(receipt_sha256)?,
            OriginState::Revoked { reason } => ensure!(
                !reason.is_empty() && reason.len() <= 256,
                "invalid revocation authority"
            ),
            OriginState::Active => (),
        }
    }
    ensure!(
        c.created_at <= c.last_time
            && c.closed_at.is_none_or(|t| t <= c.last_time)
            && (c.cursor == 0 || c.cursor < c.members.len() * OUTBOX_SLOTS),
        "invalid cohort time or cursor"
    );
    ensure!(
        c.closed_at.is_none() || c.members.iter().all(|m| m.status != OriginState::Active),
        "closed cohort has active members"
    );
    let mut ids = BTreeSet::new();
    let mut local_ids = BTreeSet::new();
    for (i, message) in c.messages.iter().enumerate() {
        message.proposal.validate()?;
        ensure!(
            message
                .proposal
                .recipients
                .iter()
                .all(|id| id != &message.task_id
                    && c.members
                        .iter()
                        .any(|member| &member.identity.task_id == id)),
            "invalid recipient authority"
        );
        let origin = c
            .members
            .iter()
            .find(|m| m.identity.run_id == message.run_id)
            .context("message has no host origin")?;
        ensure!(
            message.task_id == origin.identity.task_id
                && message.authority_sha256 == origin.identity.authority_sha256
                && message.slot < OUTBOX_SLOTS
                && message.created_at <= c.last_time
                && message.expires_at
                    == message
                        .created_at
                        .checked_add(TTL_SECONDS)
                        .context("invalid TTL")?,
            "invalid message authority"
        );
        ensure!(
            message.proposal_sha256 == storage::digest(&serde_json::to_vec(&message.proposal)?)
                && message.id
                    == message_id(
                        &c.id,
                        &origin.identity,
                        message.slot,
                        &message.proposal_sha256
                    )?,
            "message digest mismatch"
        );
        ensure!(
            ids.insert(&message.id) && local_ids.insert((&message.task_id, &message.proposal.id)),
            "duplicate committed message"
        );
        if let Some(parent_id) = &message.proposal.reply_to {
            let parent = c.messages[..i]
                .iter()
                .find(|m| &m.id == parent_id)
                .context("reply is not a prior same-cohort message")?;
            ensure!(
                message.reply_depth == parent.reply_depth + 1
                    && message.reply_depth <= MAX_REPLY_DEPTH
                    && reply_audience(&message.task_id, &message.proposal, parent),
                "invalid reply depth"
            );
        } else {
            ensure!(message.reply_depth == 0, "invalid root reply depth");
        }
    }
    for m in &c.members {
        ensure!(
            c.messages
                .iter()
                .filter(|v| v.run_id == m.identity.run_id)
                .count()
                == m.accepted,
            "run quota audit mismatch"
        );
    }
    ensure!(
        task_counts(c).values().all(|n| *n <= MAX_TASK_MESSAGES),
        "task quota audit mismatch"
    );
    presence::validate(c)?;
    Ok(())
}

pub(super) fn member_record<'a>(c: &'a Cohort, member: &HostMember) -> Result<&'a MemberRecord> {
    c.members
        .iter()
        .find(|m| m.identity == *member)
        .context("caller is not this cohort's registered host member")
}

pub(super) fn task_counts(c: &Cohort) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for m in &c.members {
        *counts.entry(m.identity.task_id.clone()).or_default() += m.accepted;
    }
    counts
}

pub(super) fn advance_time(c: &mut Cohort, now: u64) -> Result<()> {
    ensure!(now >= c.last_time, "host time moved backwards");
    c.last_time = now;
    Ok(())
}

pub(super) fn message_id(
    cohort: &str,
    origin: &HostMember,
    slot: usize,
    proposal_sha256: &str,
) -> Result<String> {
    Ok(storage::digest(&serde_json::to_vec(&(
        cohort,
        &origin.task_id,
        &origin.run_id,
        &origin.authority_sha256,
        slot,
        proposal_sha256,
    ))?))
}
