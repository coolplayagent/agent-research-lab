//! Copy only locked public crates.io archives and selected sparse index entries.
use crate::{
    REGISTRY,
    copying::Seeder,
    files::{bounded_entries, checked_directory, read_bounded},
    metadata::{safe_component, sha256},
};
use anyhow::{Context, Result, ensure};
use std::{collections::BTreeSet, path::Path};

pub(super) fn seed(seed: &mut Seeder, worktree: &Path, logs: &Path, host: &Path) -> Result<()> {
    let lock = worktree.join("Cargo.lock");
    if !lock.exists() {
        return Ok(());
    }
    let lock: toml::Value =
        toml::from_str(std::str::from_utf8(&read_bounded(&lock, 8 * 1024 * 1024)?)?)?;
    let packages = lock
        .get("package")
        .and_then(toml::Value::as_array)
        .context("Cargo.lock lacks packages")?;
    let indexes = host.join("registry/index");
    if !indexes.exists() {
        return Ok(());
    }
    checked_directory(&indexes, false)?;
    for entry in bounded_entries(&indexes)? {
        let registry = entry.file_name();
        let registry = registry.to_str().context("non-UTF8 registry name")?;
        if !registry.starts_with("index.crates.io-") || !safe_component(registry) {
            continue;
        }
        checked_directory(&entry.path(), false)?;
        let destination = logs.join("home/.cargo/registry");
        let mut names = BTreeSet::new();
        for package in packages {
            if package.get("source").and_then(toml::Value::as_str) != Some(REGISTRY) {
                continue;
            }
            let name = package
                .get("name")
                .and_then(toml::Value::as_str)
                .context("crate name missing")?;
            let version = package
                .get("version")
                .and_then(toml::Value::as_str)
                .context("crate version missing")?;
            let checksum = package
                .get("checksum")
                .and_then(toml::Value::as_str)
                .context("crate checksum missing")?;
            ensure!(
                safe_component(name) && safe_component(version) && sha256(checksum),
                "invalid locked crate identity"
            );
            names.insert(name.to_ascii_lowercase());
            let archive = format!("{name}-{version}.crate");
            let source = host.join("registry/cache").join(registry).join(&archive);
            if source.exists() {
                seed.copy(
                    &source,
                    &destination.join("cache").join(registry).join(&archive),
                    Some(checksum),
                    false,
                )?;
                seed.report.cargo_archives += 1;
            } else {
                seed.report.missing_cargo_archives.push(archive);
            }
        }
        let index_destination = destination.join("index").join(registry);
        checked_directory(&index_destination, true)?;
        // Fixed public endpoints, never an imported Cargo configuration.
        storage::write(
            &index_destination.join("config.json"),
            &serde_json::json!({"dl":"https://static.crates.io/crates","api":"https://crates.io"}),
        )?;
        for name in names {
            let suffix = match name.len() {
                1 => format!("1/{name}"),
                2 => format!("2/{name}"),
                3 => format!("3/{}/{name}", &name[..1]),
                _ => format!("{}/{}/{name}", &name[..2], &name[2..4]),
            };
            let relative = Path::new(".cache").join(suffix);
            let source = entry.path().join(&relative);
            if source.exists() {
                seed.copy(&source, &index_destination.join(relative), None, false)?;
                seed.report.cargo_index_entries += 1;
            }
        }
    }
    Ok(())
}
