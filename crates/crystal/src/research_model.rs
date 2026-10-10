//! Research coordination metadata; external knowledge stays in SuperPOD.
use contracts::{SubjectKind, TopicState};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ResearchTopic {
    pub id: String,
    pub group_id: String,
    pub title: String,
    pub objective: String,
    pub state: TopicState,
    pub revision: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ResearchSubject {
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
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EvolutionInput {
    pub id: String,
    pub subject_id: String,
    pub parents: Vec<String>,
    pub version: String,
    pub reference: String,
    pub goal_id: Option<String>,
    pub pins: EvidencePins,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EvolutionNode {
    pub input: EvolutionInput,
    pub generation: u64,
    pub created_ms: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum ResearchChange {
    Topic { topic: ResearchTopic },
    Subject { subject: ResearchSubject },
    Node { node: EvolutionInput },
}
