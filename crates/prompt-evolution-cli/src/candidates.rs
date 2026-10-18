//! Candidate registration, portfolio validation and quarantined rule proposals.
use crate::{
    files::{read_strict, write_optional},
    paths::relative_to,
    protocol::{PortfolioValidation, PromptRegistration, RuleProposalRequest},
};
use anyhow::{Context, Result, ensure};
use evolution::{PromptRegistry, PromptVersion, QuarantinedRuleProposal};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

pub(super) fn register(input: &Path, output: Option<PathBuf>) -> Result<Value> {
    let request: PromptRegistration = read_strict(input)?;
    let destination =
        output.context("prompt-register requires an explicit --output registry path")?;
    ensure!(
        destination.as_path() != input,
        "registry output cannot replace its request file"
    );
    let _lock = storage::lock(&destination.with_extension("evolution.lock"))?;
    let mut registry: PromptRegistry = if destination.exists() {
        read_strict(&destination)?
    } else {
        PromptRegistry::default()
    };
    registry.validate()?;
    let prompt = PromptVersion::new(request.definition)?;
    let already_registered = registry.get(prompt.version()).is_some();
    let version = registry.register(prompt)?;
    storage::write(&destination, &registry)?;
    Ok(
        json!({"version":version,"registry_file":destination,"already_registered":already_registered}),
    )
}

pub(super) fn portfolio(input: &Path, base: &Path, output: Option<PathBuf>) -> Result<Value> {
    let request: PortfolioValidation = read_strict(input)?;
    let registry_path = relative_to(base, &request.registry_file);
    let registry: PromptRegistry = read_strict(&registry_path)?;
    request.portfolio.validate(&registry)?;
    let result = json!({"valid":true,"activated":false,"portfolio":request.portfolio});
    write_optional(output.as_deref(), &result)?;
    Ok(result)
}

pub(super) fn rule_proposal(input: &Path, output: Option<PathBuf>) -> Result<Value> {
    let request: RuleProposalRequest = read_strict(input)?;
    let proposal = QuarantinedRuleProposal::new(
        request.parent_policy_version,
        request.proposed_rule,
        request.reason,
    )?;
    let result = json!({"quarantined":true,"adopted":false,"proposal":proposal});
    write_optional(output.as_deref(), &result)?;
    Ok(result)
}

#[cfg(test)]
#[path = "candidates_tests.rs"]
mod tests;
