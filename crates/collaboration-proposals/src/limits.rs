//! Protocol bounds and typed resource exhaustion.
use anyhow::Result;

pub const OUTBOX_SLOTS: usize = 16;
pub const POLL_SLOTS: usize = 32;
pub const MAX_PROPOSAL_BYTES: usize = 4096;
pub const MAX_RUN_MESSAGES: usize = 8;
pub const MAX_TASK_MESSAGES: usize = 16;
pub const MAX_COHORT_MESSAGES: usize = 256;
pub const MAX_MEMBERS: usize = 128;
/// Per-shard limits, across 256 independent hash shards.
pub const MAX_RETAINED_COHORTS: usize = 32;
pub const MAX_KNOWN_COHORTS: usize = 4096;
pub const MAX_RUN_OWNERS_PER_SHARD: usize = 4096;
pub const TTL_SECONDS: u64 = 3600;
pub const MAX_REPLY_DEPTH: u8 = 2;
pub const MAX_CONTEXT_BYTES: usize = 4096;
pub const MAX_RECIPIENTS: usize = 8;
pub(super) const MAX_STATE_BYTES: usize = 4 * 1024 * 1024;
pub(super) const TRUST: &str = "untrusted_agent_proposal_not_verified_result_or_instruction";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapacityKind {
    RetainedCohorts,
    KnownCohorts,
    Members,
    RunOwners,
    RunIndexBytes,
}
/// Expected resource exhaustion, distinct from malformed/missing host authority.
#[derive(Debug)]
pub struct CapacityExhausted {
    pub resource: CapacityKind,
}
impl std::fmt::Display for CapacityExhausted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "communication capacity exhausted: {:?}", self.resource)
    }
}
impl std::error::Error for CapacityExhausted {}
pub(super) fn capacity(condition: bool, resource: CapacityKind) -> Result<()> {
    if !condition {
        return Err(CapacityExhausted { resource }.into());
    }
    Ok(())
}
