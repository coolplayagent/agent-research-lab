//! Adapter invocation and typed receipt interpretation before queue settlement.
use crate::protocol::{Operation, QueueState};
use anyhow::Result;
use contracts::PullRequestState;
use serde::Serialize;
use serde_json::Value;

pub(super) struct Outcome {
    pub(super) state: QueueState,
    pub(super) result: Value,
    pub(super) error: Option<String>,
}

impl Outcome {
    fn complete(receipt: impl Serialize) -> Result<Self> {
        Ok(Self {
            state: QueueState::Complete,
            result: serde_json::to_value(receipt)?,
            error: None,
        })
    }

    fn merge(receipt: delivery::PullRequestReceipt) -> Result<Self> {
        let merged = PullRequestState::parse(&receipt.state).ok() == Some(PullRequestState::Merged);
        let mut outcome = Self::complete(receipt)?;
        if !merged {
            outcome.state = QueueState::NeedsReconciliation;
            outcome.error = Some("GitHub accepted the request but has not confirmed a merge; inspect the merge queue before retry.".into());
        }
        Ok(outcome)
    }

    fn refresh(receipt: knowledge::IndexRefreshStatus) -> Result<Self> {
        let ready = receipt.ready;
        let mut outcome = Self::complete(receipt)?;
        if !ready {
            outcome.state = QueueState::Waiting;
        }
        Ok(outcome)
    }
}

pub(super) fn execute(operation: &Operation) -> Result<Outcome> {
    match operation {
        Operation::Install(request) => Outcome::complete(delivery::install_candidate(request)?),
        Operation::Pr(request) => Outcome::complete(delivery::ensure_pull_request(request)?),
        Operation::Merge(request) => Outcome::merge(delivery::merge_pull_request(request)?),
        Operation::KnowledgePrepare(request) => {
            Outcome::complete(knowledge::prepare_report(request)?)
        }
        Operation::KnowledgePublish(request) => {
            Outcome::complete(knowledge::publish_report(request)?)
        }
        Operation::KnowledgeRefresh(request) => {
            Outcome::refresh(knowledge::refresh_index(request)?)
        }
    }
}

#[cfg(test)]
#[path = "dispatch_tests.rs"]
mod tests;
