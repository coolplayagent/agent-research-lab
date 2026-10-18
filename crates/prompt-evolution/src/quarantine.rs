//! Content-bound rule proposals that never authorize automatic policy adoption.
use crate::identity::{digest, nonempty};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

/// A proposed rule is permanently quarantined here. There is intentionally no
/// method to adopt it as formal policy. Adoption requires the repository review
/// and release path outside the self-improvement experiment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuarantinedRuleProposal {
    pub proposal_id: String,
    pub parent_policy_version: String,
    pub proposed_rule: String,
    pub reason: String,
}

impl QuarantinedRuleProposal {
    pub fn new(
        parent_policy_version: String,
        proposed_rule: String,
        reason: String,
    ) -> Result<Self> {
        nonempty(&parent_policy_version, "parent policy version")?;
        nonempty(&proposed_rule, "proposed rule")?;
        nonempty(&reason, "rule change reason")?;
        let proposal_id = digest(&(&parent_policy_version, &proposed_rule, &reason))?;
        Ok(Self {
            proposal_id,
            parent_policy_version,
            proposed_rule,
            reason,
        })
    }

    pub fn validate(&self) -> Result<()> {
        let rebuilt = Self::new(
            self.parent_policy_version.clone(),
            self.proposed_rule.clone(),
            self.reason.clone(),
        )?;
        ensure!(
            rebuilt.proposal_id == self.proposal_id,
            "rule proposal hash mismatch"
        );
        Ok(())
    }

    pub fn can_adopt_automatically(&self) -> bool {
        false
    }
}

#[cfg(test)]
#[path = "quarantine_tests.rs"]
mod tests;
