//! Resolve only task-reachable Bazel hashes and the pinned Bazelisk binary.
use crate::{
    MAX_FILES, REGISTRY,
    copying::Seeder,
    files::{bounded_entries, checked_directory, read_bounded},
    metadata::{json_hashes, safe_component, sha256},
    python_toolchain::python_toolchain_hashes,
    rust_toolchain::rust_toolchain_hashes,
};
use anyhow::{Context, Result, ensure};
use std::{collections::BTreeSet, path::Path};

pub(super) fn seed(seed: &mut Seeder, worktree: &Path, logs: &Path, host: &Path) -> Result<()> {
    if !worktree.join("MODULE.bazel").exists() {
        return Ok(());
    }
    let bazel = host.join("bazel");
    if bazel.exists() {
        checked_directory(&bazel, false)?;
        for user in bounded_entries(&bazel)? {
            let name = user.file_name();
            let name = name.to_str().context("non-UTF8 Bazel cache name")?;
            if !name.starts_with("_bazel_") || !safe_component(name) {
                continue;
            }
            let relative = Path::new("bazel")
                .join(name)
                .join("cache/repos/v1/content_addressable/sha256");
            let source = host.join(&relative);
            if !source.exists() {
                continue;
            }
            checked_directory(&source, false)?;
            // Resolve only hashes reachable from this task's frozen lock
            // inputs. Never enumerate/import another project's CAS data.
            for digest in bazel_hashes(worktree, &source)? {
                seed.deadline()?;
                let archive = source.join(&digest).join("file");
                if !archive.exists() {
                    seed.report.missing_bazel_hashes.push(digest);
                    continue;
                }
                seed.copy(
                    &archive,
                    &logs
                        .join("cache")
                        .join(&relative)
                        .join(&digest)
                        .join("file"),
                    Some(&digest),
                    false,
                )?;
                seed.report.bazel_archives += 1;
            }
        }
    }
    let version = worktree.join(".bazelversion");
    if !version.exists() {
        return Ok(());
    }
    let version = String::from_utf8(read_bounded(&version, 128)?)?;
    let version = version.trim();
    ensure!(safe_component(version), "invalid Bazel version");
    let platform = match std::env::consts::ARCH {
        "x86_64" => "x86_64",
        "aarch64" => "arm64",
        _ => return Ok(()),
    };
    let relative = Path::new("bazelisk/downloads/metadata/bazelbuild")
        .join(format!("bazel-{version}-linux-{platform}"));
    let metadata = host.join(&relative);
    if metadata.exists() {
        let digest = String::from_utf8(read_bounded(&metadata, 128)?)?;
        let digest = digest.trim();
        ensure!(sha256(digest), "invalid Bazelisk binary digest");
        let binary = Path::new("bazelisk/downloads/sha256")
            .join(digest)
            .join("bin/bazel");
        seed.copy(
            &host.join(&binary),
            &logs.join("cache").join(&binary),
            Some(digest),
            true,
        )?;
        seed.copy(&metadata, &logs.join("cache").join(&relative), None, false)?;
        seed.report.bazel_binaries += 1;
    }
    Ok(())
}

fn bazel_hashes(worktree: &Path, cas: &Path) -> Result<BTreeSet<String>> {
    let mut hashes = BTreeSet::new();
    let cargo = worktree.join("Cargo.lock");
    if cargo.exists() {
        let lock: toml::Value = toml::from_str(std::str::from_utf8(&read_bounded(
            &cargo,
            8 * 1024 * 1024,
        )?)?)?;
        for package in lock
            .get("package")
            .and_then(toml::Value::as_array)
            .context("Cargo.lock lacks packages")?
        {
            if package.get("source").and_then(toml::Value::as_str) == Some(REGISTRY) {
                let digest = package
                    .get("checksum")
                    .and_then(toml::Value::as_str)
                    .context("crate checksum missing")?;
                ensure!(sha256(digest), "invalid locked crate checksum");
                hashes.insert(digest.to_owned());
            }
        }
    }
    let module = worktree.join("MODULE.bazel.lock");
    if module.exists() {
        let lock: serde_json::Value =
            serde_json::from_slice(&read_bounded(&module, 8 * 1024 * 1024)?)?;
        if let Some(registry) = lock
            .get("registryFileHashes")
            .and_then(serde_json::Value::as_object)
        {
            for (url, digest) in registry {
                let Some(digest) = digest.as_str().filter(|v| sha256(v)) else {
                    continue;
                };
                // Only the public registry participates in host cache reuse.
                if !url.starts_with("https://bcr.bazel.build/") {
                    continue;
                }
                hashes.insert(digest.to_owned());
                if url.ends_with("/source.json") {
                    let metadata = cas.join(digest).join("file");
                    if metadata.exists() {
                        let bytes = read_bounded(&metadata, 1024 * 1024)?;
                        ensure!(
                            storage::digest(&bytes) == digest,
                            "Bazel registry metadata checksum mismatch"
                        );
                        let metadata = serde_json::from_slice(&bytes)?;
                        json_hashes(&metadata, &mut hashes)?;
                        if url.contains("/modules/rules_rust/") {
                            rust_toolchain_hashes(worktree, cas, &metadata, &mut hashes)?;
                        } else if url.contains("/modules/rules_python/") {
                            python_toolchain_hashes(worktree, cas, &metadata, &mut hashes)?;
                        }
                    }
                }
            }
        }
        if let Some(extensions) = lock.get("moduleExtensions") {
            json_hashes(extensions, &mut hashes)?;
        }
    }
    ensure!(
        hashes.len() <= MAX_FILES,
        "too many locked dependency hashes"
    );
    Ok(hashes)
}
