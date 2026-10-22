//! Research coordination metadata; knowledge stays in the optional external repository.
use contracts::{SubjectKind, TopicState};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CollaborationTopic {
    pub id: String,
    pub group_id: String,
    pub title: String,
    pub objective: String,
    pub state: TopicState,
    pub revision: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CollaborationSubject {
    pub id: String,
    pub topic_id: String,
    pub name: String,
    pub kind: SubjectKind,
    pub reference: String,
    pub revision: u64,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EvidencePins {
    pub source: Option<String>,
    pub prompt: Option<String>,
    pub policy: Option<String>,
    pub superpod: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum ResearchChange {
    Topic { topic: CollaborationTopic },
    Subject { subject: CollaborationSubject },
}

// Compatibility type names for earlier source consumers.
pub use CollaborationSubject as ResearchSubject;
pub use CollaborationTopic as ResearchTopic;
