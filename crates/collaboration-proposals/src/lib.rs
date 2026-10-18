//! Host-authorized, bounded cohort proposals. This is not a task or knowledge store.
mod admission;
mod audience;
mod authority;
mod board;
mod files;
mod limits;
mod presence;
mod projection;
mod protocol;
mod store;

pub use board::Board;
pub use contracts::OriginState;
pub use limits::{
    CapacityExhausted, CapacityKind, MAX_COHORT_MESSAGES, MAX_CONTEXT_BYTES, MAX_KNOWN_COHORTS,
    MAX_MEMBERS, MAX_PROPOSAL_BYTES, MAX_RECIPIENTS, MAX_REPLY_DEPTH, MAX_RETAINED_COHORTS,
    MAX_RUN_MESSAGES, MAX_RUN_OWNERS_PER_SHARD, MAX_TASK_MESSAGES, OUTBOX_SLOTS, POLL_SLOTS,
    TTL_SECONDS,
};
pub use presence::Presence;
pub use protocol::{
    ContextSnapshot, Endpoint, HostMember, Kind, LaunchContext, Message, PollReport, Proposal,
    Reference, Selection,
};

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
