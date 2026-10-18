//! One audience rule for admission, retained authority and every projection.
use crate::protocol::{Message, Proposal};
use std::collections::BTreeSet;

pub(super) fn visible_to(message: &Message, recipient: Option<&str>) -> bool {
    message.proposal.recipients.is_empty()
        || recipient.is_some_and(|id| {
            id == message.task_id || message.proposal.recipients.iter().any(|r| r == id)
        })
}

fn participants<'a>(sender: &'a str, proposal: &'a Proposal) -> BTreeSet<&'a str> {
    std::iter::once(sender)
        .chain(proposal.recipients.iter().map(String::as_str))
        .collect()
}

pub(super) fn reply_audience(sender: &str, proposal: &Proposal, parent: &Message) -> bool {
    parent.proposal.recipients.is_empty()
        || (!proposal.recipients.is_empty()
            && participants(sender, proposal) == participants(&parent.task_id, &parent.proposal))
}
