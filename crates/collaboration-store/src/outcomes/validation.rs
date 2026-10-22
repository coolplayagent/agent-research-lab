use super::*;

pub(super) fn bounded(value: &str, max: usize) -> Result<()> {
    check(
        !value.trim().is_empty() && value.len() <= max && !value.contains('\0'),
        Code::Invalid,
    )
}
pub(super) fn hash(value: &str, lengths: &[usize]) -> Result<()> {
    check(
        lengths.contains(&value.len())
            && value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        Code::Invalid,
    )
}
pub(super) fn validate_evidence(evidence: &[OutcomeEvidence]) -> Result<()> {
    check(!evidence.is_empty() && evidence.len() <= 16, Code::Invalid)?;
    for reference in evidence {
        bounded(&reference.repository, 1024)?;
        hash(&reference.commit, &[40, 64])?;
        hash(&reference.sha256, &[64])?;
        bounded(&reference.path, 1024)?;
        check(
            !std::path::Path::new(&reference.path).is_absolute()
                && !std::path::Path::new(&reference.path)
                    .components()
                    .any(|p| matches!(p, std::path::Component::ParentDir)),
            Code::Invalid,
        )?;
    }
    Ok(())
}
pub(super) fn validate_input(input: &OutcomeInput) -> Result<()> {
    id(&input.id)?;
    id(&input.group_id)?;
    id(&input.goal_id)?;
    bounded(&input.title, 256)?;
    bounded(
        &input.summary,
        if input.node_id.is_some() { 16000 } else { 4096 },
    )?;
    bounded(&input.conditions, 4096)?;
    if input.node_id.is_none() || !input.evidence.is_empty() {
        validate_evidence(&input.evidence)?;
    }
    check(
        input.references.len() <= if input.node_id.is_some() { 128 } else { 16 },
        Code::Invalid,
    )?;
    let mut seen = std::collections::BTreeSet::new();
    for reference in &input.references {
        id(&reference.outcome_id)?;
        check(
            seen.insert(&reference.outcome_id) && reference.outcome_id != input.id,
            Code::Invalid,
        )?;
        bounded(&reference.reason, 1024)?;
    }
    if let Some(parent) = &input.supersedes {
        id(parent)?;
        check(parent != &input.id, Code::Invalid)?;
    }
    if let Some(attempt) = &input.attempt {
        id(&attempt.goal_id)?;
        id(&attempt.work_id)?;
        id(&attempt.claim_id)?;
        check(attempt.attempt > 0, Code::Invalid)?;
    }
    hash(
        input.pins.source.as_deref().context(Code::Invalid)?,
        &[40, 64],
    )?;
    for pin in [&input.pins.prompt, &input.pins.policy] {
        hash(pin.as_deref().context(Code::Invalid)?, &[64])?;
    }
    if let Some(pin) = &input.pins.superpod {
        hash(pin, &[40, 64])?;
    }
    if let Some(binding) = &input.knowledge {
        id(&binding.service_id)?;
        bounded(&binding.repository, 1024)?;
        bounded(&binding.revision, 256)?;
    } else {
        check(input.pins.superpod.is_none(), Code::Invalid)?;
    }
    check(serde_json::to_vec(input)?.len() <= 64 * 1024, Code::Invalid)
}
pub(super) fn current_attempt(db: &Connection, entry: &Outcome) -> Result<bool> {
    let Some(binding) = &entry.input.attempt else {
        return Ok(true);
    };
    if let Some(node_id) = &entry.input.node_id {
        let task = crate::tasks::read(db, &binding.goal_id)?;
        return Ok(task.state != TaskState::Cancelled
            && task.nodes.iter().any(|n| {
                n.input.id == *node_id
                    && n.artifact_id.as_deref() == Some(&entry.input.id)
                    && n.attempt == binding.attempt
            }));
    }
    let goal = goals::read(db, &binding.goal_id)?;
    Ok(goal.state != GoalState::Cancelled
        && goal.work.iter().any(|work| {
            work.id == binding.work_id
                && work.person_id == entry.author_id
                && work.attempt == binding.attempt
                && work.claim_id.as_deref() == Some(&binding.claim_id)
        }))
}
