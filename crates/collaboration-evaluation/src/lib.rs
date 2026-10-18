//! Portable, host-owned collaboration pilots. A team pair is one observation;
//! worker claim cards and mutable traces never authorize promotion or delivery.
mod adjudication;
mod artifacts;
mod bindings;
mod cards;
mod citations;
mod collection;
mod identity;
mod metrics;
mod planning;
mod protocol;
mod registration;
mod usage;

use anyhow::{Result, bail, ensure};
use config::Config;
pub use protocol::{
    Adjudication, Adjudications, Arm, Change, Citation, ClaimCard, CollectedClaim, KnowledgeUpdate,
    Plan, PlanRequest, PlannedTask, Slot,
};
use serde_json::Value;
use std::path::Path;

/// `plan` freezes tasks; `enqueue` reuses one verified cohort through the existing
/// durable registration path. `collect` reads workflow authority. None starts workers.
pub fn execute(
    c: &Config,
    action: &str,
    input: &Path,
    output: &Path,
    grades: Option<&Path>,
) -> Result<Value> {
    match action {
        "plan" => {
            ensure!(grades.is_none(), "plan does not accept adjudications");
            planning::execute(c, input, output)
        }
        "enqueue" => {
            ensure!(grades.is_none(), "enqueue does not accept adjudications");
            registration::execute(c, input, output)
        }
        "collect" => collection::execute(c, input, output, grades),
        _ => bail!("unknown collaboration action; use plan, enqueue or collect"),
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
