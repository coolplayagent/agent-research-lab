//! Source, prompt and policy pins for durable outcomes.
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EvidencePins {
    pub source: Option<String>,
    pub prompt: Option<String>,
    pub policy: Option<String>,
    pub superpod: Option<String>,
}
