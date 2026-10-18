//! Durable host-owned delivery outbox. Agent output is never an enqueue instruction.
//! The host must keep this directory and `evidence/` outside candidate writable sandboxes.
mod dispatch;
mod protocol;
mod provenance;
mod queue;
mod store;

pub use protocol::{Operation, QueueEntry, QueueState};
pub use queue::{enqueue, reconcile_retry, tick};
