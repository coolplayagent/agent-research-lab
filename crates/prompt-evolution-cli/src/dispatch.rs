//! Normalize request/output paths once and route to one command owner.
use crate::{
    candidates, evaluation, exploration, lineage,
    paths::{self, resolve_destination},
    promotion,
};
use anyhow::{Context, Result, bail};
use freshness::RemoteClient;
use serde_json::Value;
use std::path::Path;

/// Turn a structured agent proposal into an immutable candidate. Registration
/// preserves ancestry; choosing the stable portfolio still requires the host
/// promotion path. JSON contains PromptRegistration, never an approval field.
pub fn register_prompt_candidate(proposal_file: &Path, registry_file: &Path) -> Result<Value> {
    execute("prompt-register", proposal_file, Some(registry_file))
}

/// Paths in JSON are interpreted relative to the request file, not the shell's
/// working directory. Stateful operations require an explicit output path.
pub fn execute(action: &str, input: &Path, output: Option<&Path>) -> Result<Value> {
    execute_with_client(action, input, output, &RemoteClient::default())
}

pub(super) fn execute_with_client(
    action: &str,
    input: &Path,
    output: Option<&Path>,
    remote: &RemoteClient,
) -> Result<Value> {
    let input = input
        .canonicalize()
        .with_context(|| format!("cannot read {}", input.display()))?;
    let base = input.parent().context("request file has no parent")?;
    let output = output.map(resolve_destination).transpose()?;
    match action {
        "lineage" => lineage::run(&input, base, output),
        "prompt-register" => candidates::register(&input, output),
        "prompt-portfolio" => candidates::portfolio(&input, base, output),
        "evaluate" => evaluation::promotion(&input, base, output),
        "novelty" => evaluation::novelty(&input, base, output),
        "exploration" => exploration::run(&input, base, output, remote),
        "rule-proposal" => candidates::rule_proposal(&input, output),
        "promote" => promotion::run(&input, base, output, remote),
        "holdout-validate" => paths::validate_holdout(&input, base, output),
        _ => bail!("unknown evolution action: {action}"),
    }
}
