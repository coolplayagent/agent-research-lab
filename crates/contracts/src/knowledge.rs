//! Structured knowledge diagnostics shared by services and HTTP presentation boundaries.
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KnowledgeError {
    pub code: crate::KnowledgeErrorCode,
    pub parameters: std::collections::BTreeMap<String, String>,
}
impl KnowledgeError {
    pub fn new(code: crate::KnowledgeErrorCode) -> Self {
        Self {
            code,
            parameters: Default::default(),
        }
    }
    pub fn parameter(mut self, key: &str, value: impl ToString) -> Self {
        self.parameters.insert(key.into(), value.to_string());
        self
    }
}
impl std::fmt::Display for KnowledgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.code.fmt(f)
    }
}
impl std::error::Error for KnowledgeError {}
