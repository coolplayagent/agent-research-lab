//! Persistent exploration windows with bounded shares and replay-resistant experiment IDs.
use crate::identity::nonempty;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplorationOutcome {
    pub experiment_id: String,
    pub promoted_on_holdout: bool,
    pub critical_regression: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplorationBudget {
    share_percent: u8,
    pending: Vec<ExplorationOutcome>,
    completed_experiments: BTreeSet<String>,
}

impl Default for ExplorationBudget {
    fn default() -> Self {
        Self {
            share_percent: 25,
            pending: Vec::new(),
            completed_experiments: BTreeSet::new(),
        }
    }
}

impl ExplorationBudget {
    pub fn share_percent(&self) -> u8 {
        self.share_percent
    }

    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            (10..=50).contains(&self.share_percent),
            "exploration share is outside 10..=50"
        );
        ensure!(self.pending.len() < 10, "unprocessed exploration window");
        let mut seen = BTreeSet::new();
        for outcome in &self.pending {
            nonempty(&outcome.experiment_id, "experiment ID")?;
            ensure!(
                seen.insert(&outcome.experiment_id),
                "duplicate pending experiment"
            );
            ensure!(
                self.completed_experiments.contains(&outcome.experiment_id),
                "unregistered experiment in exploration window"
            );
        }
        Ok(())
    }

    /// Returns Some(new_share) only when a complete window has been processed.
    /// Experiment IDs survive restarts and cannot be counted twice.
    pub fn record(&mut self, outcome: ExplorationOutcome) -> Result<Option<u8>> {
        self.validate()?;
        nonempty(&outcome.experiment_id, "experiment ID")?;
        ensure!(
            !self.completed_experiments.contains(&outcome.experiment_id),
            "experiment already counted"
        );
        self.completed_experiments
            .insert(outcome.experiment_id.clone());
        self.pending.push(outcome);
        if self.pending.len() < 10 {
            return Ok(None);
        }
        let promotions = self
            .pending
            .iter()
            .filter(|entry| entry.promoted_on_holdout)
            .count();
        let critical = self.pending.iter().any(|entry| entry.critical_regression);
        if critical || promotions == 0 {
            self.share_percent = self.share_percent.saturating_sub(5).max(10);
        } else if promotions >= 2 {
            self.share_percent = (self.share_percent + 5).min(50);
        }
        self.pending.clear();
        Ok(Some(self.share_percent))
    }
}

#[cfg(test)]
#[path = "exploration_tests.rs"]
mod tests;
