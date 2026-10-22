//! Host-owned deletion requests. Evidence and lineage are never cascaded away.
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DeleteTarget {
    Group {
        id: String,
        revision: u64,
    },
    Announcement {
        group_id: String,
        sequence: u64,
        revision: u64,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeletionResult {
    pub code: contracts::DeletionCode,
    pub references: Vec<String>,
}
