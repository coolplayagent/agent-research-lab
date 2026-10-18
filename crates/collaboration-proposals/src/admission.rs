//! Consume each outbox slot once and apply host-bound proposal quotas and audience.
use crate::{
    audience::reply_audience,
    authority::{Cohort, Seen, message_id, task_counts},
    files::{SlotInput, slot_input},
    limits::{
        MAX_COHORT_MESSAGES, MAX_REPLY_DEPTH, MAX_RUN_MESSAGES, MAX_TASK_MESSAGES, TTL_SECONDS,
    },
    presence::{self, Presence},
    protocol::{Message, PollReport, Proposal},
    store::Store,
};
use anyhow::{Context, Result};
use contracts::OriginState;

enum Accepted {
    New,
    Duplicate,
    Rejected(&'static str),
}

fn accept(
    c: &mut Cohort,
    member: usize,
    slot: usize,
    proposal: Proposal,
    now: u64,
) -> Result<Accepted> {
    let origin = &c.members[member].identity;
    let canonical = serde_json::to_vec(&proposal)?;
    let proposal_sha256 = storage::digest(&canonical);
    // Idempotency is checked before lifetime quotas, including after expiration.
    if let Some(old) = c
        .messages
        .iter()
        .find(|m| m.task_id == origin.task_id && m.proposal.id == proposal.id)
    {
        return Ok(if old.proposal_sha256 == proposal_sha256 {
            Accepted::Duplicate
        } else {
            Accepted::Rejected("local_id_content_conflict")
        });
    }
    if c.messages.iter().any(|m| {
        m.task_id == origin.task_id
            && m.proposal.kind == proposal.kind
            && m.proposal.text == proposal.text
            && m.proposal.topics == proposal.topics
            && m.proposal.references == proposal.references
            && m.proposal.reply_to == proposal.reply_to
            && m.proposal.recipients == proposal.recipients
    }) {
        return Ok(Accepted::Duplicate);
    }
    if c.members[member].accepted >= MAX_RUN_MESSAGES {
        return Ok(Accepted::Rejected("run_quota_exhausted"));
    }
    if task_counts(c)[&origin.task_id] >= MAX_TASK_MESSAGES {
        return Ok(Accepted::Rejected("task_quota_exhausted"));
    }
    if c.messages.len() >= MAX_COHORT_MESSAGES {
        return Ok(Accepted::Rejected("cohort_capacity_exhausted"));
    }
    if proposal
        .recipients
        .iter()
        .any(|id| id == &origin.task_id || !c.members.iter().any(|m| &m.identity.task_id == id))
    {
        return Ok(Accepted::Rejected("recipient_unavailable"));
    }
    let depth = if let Some(parent) = &proposal.reply_to {
        let Some((index, parent)) = c
            .messages
            .iter()
            .enumerate()
            .find(|(_, m)| &m.id == parent && m.expires_at > now)
        else {
            return Ok(Accepted::Rejected("reply_target_unavailable"));
        };
        if !reply_audience(&origin.task_id, &proposal, parent) {
            return Ok(Accepted::Rejected("reply_audience_mismatch"));
        }
        if !presence::delivered(c, index, origin) {
            return Ok(Accepted::Rejected("reply_target_not_delivered"));
        }
        if matches!(
            c.members
                .iter()
                .find(|m| m.identity.run_id == parent.run_id)
                .unwrap()
                .status,
            OriginState::Revoked { .. }
        ) {
            return Ok(Accepted::Rejected("reply_origin_revoked"));
        }
        if parent.reply_depth >= MAX_REPLY_DEPTH {
            return Ok(Accepted::Rejected("reply_depth_exhausted"));
        }
        parent.reply_depth + 1
    } else {
        0
    };
    let id = message_id(&c.id, origin, slot, &proposal_sha256)?;
    c.messages.push(Message {
        id,
        task_id: origin.task_id.clone(),
        run_id: origin.run_id.clone(),
        authority_sha256: origin.authority_sha256.clone(),
        slot,
        created_at: now,
        expires_at: now.checked_add(TTL_SECONDS).context("TTL overflow")?,
        reply_depth: depth,
        proposal_sha256,
        proposal,
    });
    c.members[member].accepted += 1;
    presence::speak(&mut c.members[member], now);
    Ok(Accepted::New)
}

pub(super) fn consume_slot(
    store: &Store,
    c: &mut Cohort,
    mi: usize,
    slot: usize,
    now: u64,
    report: &mut PollReport,
) -> Result<()> {
    if c.members[mi].status != OriginState::Active
        || presence::state(&c.members[mi]) == Presence::Offline
    {
        return Ok(());
    }
    report.scanned_slots += 1;
    let input = slot_input(&store.outbox(&c.members[mi].identity.run_id), slot);
    let (bytes_digest, proposal, invalid) = match input {
        SlotInput::Missing => return Ok(()),
        SlotInput::Invalid(reason, digest) => (digest, None, Some(reason)),
        SlotInput::Proposal(proposal, digest) => (Some(digest), Some(proposal), None),
    };
    if let Some(old) = c.members[mi].slots.get_mut(&slot) {
        if old.digest != bytes_digest && !old.changed_after_consumption {
            old.changed_after_consumption = true;
            report.changed_consumed_slots += 1;
        }
        return Ok(());
    }
    let outcome = if let Some(reason) = invalid {
        report.rejected += 1;
        reason.to_owned()
    } else {
        match accept(c, mi, slot, proposal.unwrap(), now)? {
            Accepted::New => {
                report.accepted += 1;
                "accepted".into()
            }
            Accepted::Duplicate => {
                report.duplicates += 1;
                "duplicate".into()
            }
            Accepted::Rejected(reason) => {
                report.rejected += 1;
                reason.into()
            }
        }
    };
    c.members[mi].slots.insert(
        slot,
        Seen {
            digest: bytes_digest,
            outcome,
            changed_after_consumption: false,
        },
    );
    Ok(())
}
