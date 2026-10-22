//! Runtime storage accepts only the current schema; upgrades are offline.
use anyhow::{Context, Result, ensure};
use rusqlite::Connection;
pub(crate) fn validate(connection: &Connection) -> Result<()> {
    let version: u64 = connection
        .query_row(
            "SELECT value FROM crystal_meta WHERE key='schema'",
            [],
            |r| r.get(0),
        )
        .context("migration_required: use outcome-hub-migrate on an offline copy")?;
    ensure!(
        version == 8,
        "migration_required: unsupported storage schema {version}; use outcome-hub-migrate"
    );
    Ok(())
}
