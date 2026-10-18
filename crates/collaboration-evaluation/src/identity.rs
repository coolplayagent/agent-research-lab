//! Stable evidence identity; only freshness observation timestamps are normalized.
use anyhow::Result;
use inputs::ExecutionInputs;
use serde::Serialize;

pub(super) fn digest(value: &impl Serialize) -> Result<String> {
    Ok(storage::digest(&serde_json::to_vec(value)?))
}

pub(super) fn input_digest(inputs: &ExecutionInputs) -> Result<String> {
    let mut normalized = inputs.clone();
    for repository in normalized.repositories.values_mut() {
        repository.upstream.checked_at.clear();
    }
    for skill in &mut normalized.skills.skills {
        skill.checked_at.clear();
    }
    digest(&normalized)
}
