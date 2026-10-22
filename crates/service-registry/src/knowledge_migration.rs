//! Remove retired external knowledge connections without touching knowledge data or history.
use anyhow::{Result, ensure};
use contracts::{OutcomeServiceKind, ServiceManagement};
use service_api::Configuration;
use std::{collections::BTreeSet, path::Path};

pub(super) fn migrate(path: &Path, config: &mut Configuration) -> Result<()> {
    let removed = config
        .outcome_services
        .iter()
        .filter(|s| {
            s.kind == OutcomeServiceKind::Knowledge && s.management == ServiceManagement::External
        })
        .map(|s| s.id.clone())
        .collect::<BTreeSet<_>>();
    if removed.is_empty() {
        return Ok(());
    }
    let original = serde_json::to_vec(config)?;
    let digest = storage::digest(&original);
    let backup = path.with_file_name(format!(
        "knowledge-external-before-{}-{}.json",
        config.revision,
        &digest[..16]
    ));
    if backup.exists() {
        let saved: Configuration = storage::read(&backup)?;
        ensure!(
            serde_json::to_vec(&saved)? == original,
            "knowledge_migration_backup_conflict"
        );
    } else {
        storage::write(&backup, config)?;
    }
    let mut next = config.clone();
    next.outcome_services.retain(|s| !removed.contains(&s.id));
    next.knowledge_groups
        .retain(|_, b| !removed.contains(&b.service_id));
    next.knowledge_goals
        .retain(|_, b| !removed.contains(&b.service_id));
    next.revision = next
        .revision
        .checked_add(1)
        .ok_or_else(|| anyhow::anyhow!("configuration revision exhausted"))?;
    next.validate()?;
    storage::write(
        &path.with_file_name("knowledge-external-migration.json"),
        &serde_json::json!({"previous_revision":config.revision,"configuration_revision":next.revision,"removed_services":removed,"backup":backup.file_name().unwrap().to_string_lossy(),"previous_sha256":digest}),
    )?;
    storage::write(path, &next)?;
    *config = next;
    Ok(())
}

#[cfg(test)]
#[path = "knowledge_migration_tests.rs"]
mod tests;
