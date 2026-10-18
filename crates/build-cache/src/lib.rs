//! Dependency cache copies are a performance input, never evaluation evidence.
//! Each task owns its writable copy. Only checksum-addressed archives and the
//! public crates.io sparse index are copied; credentials, configuration, locks,
//! extracted repositories and compiler output are never imported.

use anyhow::{Context, Result, ensure};
use native_effects::fs::{self, File, OpenOptions};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    io::Read,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

const MAX_FILES: usize = 4096;
const MAX_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_FILE_BYTES: u64 = 512 * 1024 * 1024;
const REGISTRY: &str = "registry+https://github.com/rust-lang/crates.io-index";
static SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SeedReport {
    schema_version: u32,
    inputs_digest: String,
    pub cargo_archives: usize,
    pub cargo_index_entries: usize,
    pub bazel_archives: usize,
    pub bazel_binaries: usize,
    pub missing_cargo_archives: Vec<String>,
    pub copied_files: usize,
    pub copied_bytes: u64,
    pub reused_files: usize,
    pub retained_files: usize,
    pub retained_bytes: u64,
    pub missing_bazel_hashes: Vec<String>,
    pub elapsed_millis: u64,
}

#[derive(Debug)]
pub struct PreparationDeferred;
impl std::fmt::Display for PreparationDeferred {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("build cache preparation deadline exhausted; private copies can be resumed")
    }
}
impl std::error::Error for PreparationDeferred {}

pub fn deferred(error: &anyhow::Error) -> bool {
    error.downcast_ref::<PreparationDeferred>().is_some()
}

pub fn applicable(worktree: &Path) -> bool {
    worktree.join("Cargo.lock").is_file() || worktree.join("MODULE.bazel").is_file()
}

fn inputs_digest(worktree: &Path) -> Result<String> {
    let mut digest = Sha256::new();
    for name in [
        "Cargo.lock",
        "MODULE.bazel",
        "MODULE.bazel.lock",
        ".bazelversion",
        "rust-toolchain.toml",
    ] {
        digest.update(name);
        let path = worktree.join(name);
        if path.exists() {
            digest.update(read_bounded(&path, 8 * 1024 * 1024)?);
        }
    }
    Ok(format!("{:x}", digest.finalize()))
}

/// A preparation marker only avoids repeated copies; build tools still check
/// their lockfiles and archives. It never authorizes a gate or an agent effect.
pub fn ready(worktree: &Path, logs: &Path) -> Result<bool> {
    let marker = logs.join("build-cache-seed.json");
    if !marker.exists() {
        return Ok(false);
    }
    let report: SeedReport = serde_json::from_slice(&read_bounded(&marker, 1024 * 1024)?)?;
    Ok(report.schema_version == 1 && report.inputs_digest == inputs_digest(worktree)?)
}

/// Run before effect_claim, with no active workers, inside the host's accounted
/// prerequisite allowance. A deadline leaves resumable private cache files.
pub fn prepare(worktree: &Path, logs: &Path) -> Result<SeedReport> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let cargo = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| home.as_ref().map(|p| p.join(".cargo")));
    let cache = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| home.as_ref().map(|p| p.join(".cache")));
    prepare_from(worktree, logs, cargo.as_deref(), cache.as_deref())
}

fn prepare_from(
    worktree: &Path,
    logs: &Path,
    cargo: Option<&Path>,
    cache: Option<&Path>,
) -> Result<SeedReport> {
    checked_directory(worktree, false)?;
    checked_directory(logs, true)?;
    let mut seed = Seeder {
        started: Instant::now(),
        report: SeedReport {
            schema_version: 1,
            inputs_digest: inputs_digest(worktree)?,
            ..SeedReport::default()
        },
    };
    if let Some(cargo) = cargo.filter(|p| p.is_dir()) {
        seed.cargo(worktree, logs, cargo)?;
    }
    if let Some(cache) = cache.filter(|p| p.is_dir()) {
        seed.bazel(worktree, logs, cache)?;
    }
    seed.report.elapsed_millis = seed.started.elapsed().as_millis() as u64;
    storage::write(&logs.join("build-cache-seed.json"), &seed.report)?;
    Ok(seed.report)
}

struct Seeder {
    started: Instant,
    report: SeedReport,
}

impl Seeder {
    fn deadline(&self) -> Result<()> {
        if process::deadline_exhausted() || self.started.elapsed() >= Duration::from_secs(55) {
            return Err(PreparationDeferred.into());
        }
        Ok(())
    }

    fn cargo(&mut self, worktree: &Path, logs: &Path, host: &Path) -> Result<()> {
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
                    self.copy(
                        &source,
                        &destination.join("cache").join(registry).join(&archive),
                        Some(checksum),
                        false,
                    )?;
                    self.report.cargo_archives += 1;
                } else {
                    self.report.missing_cargo_archives.push(archive);
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
                    self.copy(&source, &index_destination.join(relative), None, false)?;
                    self.report.cargo_index_entries += 1;
                }
            }
        }
        Ok(())
    }

    fn bazel(&mut self, worktree: &Path, logs: &Path, host: &Path) -> Result<()> {
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
                    self.deadline()?;
                    let archive = source.join(&digest).join("file");
                    if !archive.exists() {
                        self.report.missing_bazel_hashes.push(digest);
                        continue;
                    }
                    self.copy(
                        &archive,
                        &logs
                            .join("cache")
                            .join(&relative)
                            .join(&digest)
                            .join("file"),
                        Some(&digest),
                        false,
                    )?;
                    self.report.bazel_archives += 1;
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
            self.copy(
                &host.join(&binary),
                &logs.join("cache").join(&binary),
                Some(digest),
                true,
            )?;
            self.copy(&metadata, &logs.join("cache").join(&relative), None, false)?;
            self.report.bazel_binaries += 1;
        }
        Ok(())
    }

    fn copy(
        &mut self,
        source: &Path,
        destination: &Path,
        expected: Option<&str>,
        executable: bool,
    ) -> Result<()> {
        self.deadline()?;
        let mut input = regular_file(source)?;
        let size = input.metadata()?.len();
        ensure!(
            size <= MAX_FILE_BYTES
                && self.report.retained_files < MAX_FILES
                && self.report.retained_bytes + size <= MAX_BYTES,
            "build cache copy budget exceeded"
        );
        self.report.retained_files += 1;
        self.report.retained_bytes += size;
        checked_directory(
            destination
                .parent()
                .context("cache destination lacks parent")?,
            true,
        )?;
        if destination.exists() {
            let expected = if let Some(expected) = expected {
                expected.to_owned()
            } else {
                self.hash(source)?
            };
            if self.hash(destination)? == expected {
                self.report.reused_files += 1;
                return Ok(());
            }
        }
        let temporary = destination.with_extension(format!(
            "seed-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let result = (|| -> Result<()> {
            fs::check_write_bound(&temporary, size)?;
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW)
                .open(&temporary)?;
            let mut hash = Sha256::new();
            let mut buffer = [0u8; 64 * 1024];
            let mut copied = 0u64;
            loop {
                self.deadline()?;
                let count = input.read(&mut buffer)?;
                if count == 0 {
                    break;
                }
                copied += count as u64;
                ensure!(copied <= size, "cache source grew during copy");
                fs::write_all(&mut output, &temporary, &buffer[..count])?;
                hash.update(&buffer[..count]);
            }
            ensure!(copied == size, "cache source changed during copy");
            let digest = format!("{:x}", hash.finalize());
            ensure!(
                expected.is_none_or(|expected| digest == expected),
                "cache checksum mismatch: {}",
                source.display()
            );
            if executable {
                fs::set_file_permissions(&output, &temporary, fs::Permissions::from_mode(0o700))?;
            }
            drop(output);
            fs::rename(&temporary, destination)?;
            Ok(())
        })();
        if result.is_err() {
            let _cleanup = native_effects::cleanup_scope();
            let _ = fs::remove_file(&temporary);
        }
        result?;
        self.report.copied_files += 1;
        self.report.copied_bytes += size;
        Ok(())
    }

    fn hash(&self, path: &Path) -> Result<String> {
        let mut file = regular_file(path)?;
        let size = file.metadata()?.len();
        ensure!(size <= MAX_FILE_BYTES, "cache file exceeds byte limit");
        let mut hash = Sha256::new();
        let mut buffer = [0u8; 64 * 1024];
        let mut count = 0;
        loop {
            self.deadline()?;
            let n = file.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            count += n as u64;
            ensure!(count <= size, "cache source grew during verification");
            hash.update(&buffer[..n]);
        }
        ensure!(count == size, "cache source changed during verification");
        Ok(format!("{:x}", hash.finalize()))
    }
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

fn json_hashes(value: &serde_json::Value, hashes: &mut BTreeSet<String>) -> Result<()> {
    match value {
        serde_json::Value::Object(object) => {
            for (key, value) in object {
                if key == "sha256" {
                    if let Some(digest) = value.as_str().filter(|v| sha256(v)) {
                        hashes.insert(digest.to_owned());
                    }
                } else if key == "integrity" {
                    if let Some(integrity) = value.as_str() {
                        for item in integrity.split_whitespace() {
                            if let Some(encoded) = item.strip_prefix("sha256-") {
                                hashes.insert(decode_sha256(encoded)?);
                            }
                        }
                    }
                } else if key == "patches"
                    && let Some(patches) = value.as_object()
                {
                    for digest in patches.values().filter_map(serde_json::Value::as_str) {
                        if sha256(digest) {
                            hashes.insert(digest.to_owned());
                        } else if let Some(encoded) = digest.strip_prefix("sha256-") {
                            hashes.insert(decode_sha256(encoded)?);
                        }
                    }
                }
                json_hashes(value, hashes)?;
            }
        }
        serde_json::Value::Array(array) => {
            for value in array {
                json_hashes(value, hashes)?;
            }
        }
        _ => {}
    }
    ensure!(hashes.len() <= MAX_FILES, "too many dependency hashes");
    Ok(())
}

fn decode_sha256(encoded: &str) -> Result<String> {
    ensure!(
        encoded.len() == 44 && encoded.ends_with('='),
        "invalid SHA256 integrity length"
    );
    let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut bits = 0u32;
    let mut count = 0;
    let mut bytes = Vec::new();
    for byte in encoded.bytes().take(43) {
        let value = alphabet
            .iter()
            .position(|b| *b == byte)
            .context("invalid SHA256 integrity encoding")? as u32;
        bits = (bits << 6) | value;
        count += 6;
        if count >= 8 {
            count -= 8;
            bytes.push((bits >> count) as u8);
        }
    }
    ensure!(
        bytes.len() == 32 && bits & 3 == 0,
        "invalid SHA256 integrity padding"
    );
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

fn rust_toolchain_hashes(
    worktree: &Path,
    cas: &Path,
    metadata: &serde_json::Value,
    hashes: &mut BTreeSet<String>,
) -> Result<()> {
    let Some(archive) = verified_archive(cas, metadata)? else {
        return Ok(());
    };
    let member = |name: &str| read_tar_member(worktree, &archive, &format!("./{name}"));
    // These are the literal archive declarations of rules_rust's non-dev
    // internal extensions. Read only these named metadata files from the
    // checksum-verified distribution; never run candidate Starlark on the host.
    for path in [
        "rust/private/repository_utils.bzl",
        "cargo/3rdparty/crates/crates.bzl",
    ] {
        literal_archive_hashes(&member(path)?, hashes)?;
    }
    let common = member("rust/private/common.bzl")?;
    let constant = |name: &str| -> Result<String> {
        let value = common
            .lines()
            .filter_map(|line| line.split_once('='))
            .find(|(key, _)| key.trim() == name)
            .context("rules_rust default missing")?
            .1
            .trim();
        Ok(value
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .context("rules_rust default must be a literal")?
            .to_owned())
    };
    let mut versions = BTreeSet::from([constant("DEFAULT_RUST_VERSION")?]);
    let nightly = constant("DEFAULT_NIGHTLY_ISO_DATE")?;
    let config = worktree.join("rust-toolchain.toml");
    if config.exists() {
        let config: toml::Value =
            toml::from_str(std::str::from_utf8(&read_bounded(&config, 1024 * 1024)?)?)?;
        if let Some(version) = config
            .get("toolchain")
            .and_then(|v| v.get("channel"))
            .and_then(toml::Value::as_str)
        {
            ensure!(
                !version.is_empty() && version.bytes().all(|b| b.is_ascii_digit() || b == b'.'),
                "cache reuse requires a pinned Rust version"
            );
            let module =
                String::from_utf8(read_bounded(&worktree.join("MODULE.bazel"), 1024 * 1024)?)?;
            let module: String = module.chars().filter(|c| !c.is_whitespace()).collect();
            if module.contains(&format!("versions=[\"{version}\"]")) {
                versions.insert(version.to_owned());
            }
        }
    }
    let triple = format!("{}-unknown-linux-gnu", std::env::consts::ARCH);
    let tools = ["rustc", "rust-std", "cargo", "clippy", "llvm-tools"];
    let known = member("rust/private/known_shas.bzl")?;
    for line in known.lines() {
        let fields: Vec<_> = line.split('"').collect();
        if fields.len() < 4 || !sha256(fields[3]) {
            continue;
        }
        let name = fields[1];
        let stable = versions.iter().any(|version| {
            tools
                .iter()
                .any(|tool| name == format!("{tool}-{version}-{triple}.tar.xz"))
        });
        let formatter = name == format!("{nightly}/rustfmt-nightly-{triple}.tar.xz");
        if stable || formatter {
            hashes.insert(fields[3].to_owned());
        }
    }
    Ok(())
}

fn verified_archive(cas: &Path, metadata: &serde_json::Value) -> Result<Option<PathBuf>> {
    let Some(integrity) = metadata
        .get("integrity")
        .and_then(serde_json::Value::as_str)
        .and_then(|value| value.strip_prefix("sha256-"))
    else {
        return Ok(None);
    };
    let digest = decode_sha256(integrity)?;
    let archive = cas.join(&digest).join("file");
    if !archive.exists() {
        return Ok(None);
    }
    let bytes = read_bounded(&archive, 128 * 1024 * 1024)?;
    ensure!(
        storage::digest(&bytes) == digest,
        "dependency metadata archive checksum mismatch"
    );
    Ok(Some(archive))
}

fn read_tar_member(worktree: &Path, archive: &Path, member: &str) -> Result<String> {
    let output = process::capture(
        "tar",
        &["-xOf".into(), archive.display().to_string(), member.into()],
        worktree,
        Duration::from_secs(10),
    )?;
    ensure!(
        output.status.success(),
        "cannot read verified dependency metadata"
    );
    Ok(String::from_utf8(output.stdout)?)
}

fn python_toolchain_hashes(
    worktree: &Path,
    cas: &Path,
    metadata: &serde_json::Value,
    hashes: &mut BTreeSet<String>,
) -> Result<()> {
    let Some(archive) = verified_archive(cas, metadata)? else {
        return Ok(());
    };
    let prefix = metadata
        .get("strip_prefix")
        .and_then(serde_json::Value::as_str)
        .context("rules_python archive prefix missing")?;
    ensure!(
        safe_component(prefix),
        "unsupported rules_python archive prefix"
    );
    let module = read_tar_member(worktree, &archive, &format!("{prefix}/MODULE.bazel"))?;
    let versions = read_tar_member(worktree, &archive, &format!("{prefix}/python/versions.bzl"))?;
    let triple = format!("{}-unknown-linux-gnu", std::env::consts::ARCH);
    hashes.insert(python_default_hash(&module, &versions, &triple)?);
    Ok(())
}

fn python_default_hash(module: &str, versions: &str, triple: &str) -> Result<String> {
    let default = module
        .split_once("python.defaults(")
        .and_then(|(_, value)| value.split_once(')'))
        .context("rules_python default missing")?
        .0;
    let minor = default
        .lines()
        .filter_map(|line| line.split_once('='))
        .find(|(key, _)| key.trim() == "python_version")
        .context("rules_python default version missing")?
        .1
        .trim()
        .trim_end_matches(',')
        .trim_matches('"');
    let mapping = versions
        .split_once("MINOR_MAPPING = {")
        .and_then(|(_, value)| value.split_once("\n}"))
        .context("Python minor mapping missing")?
        .0;
    let version =
        literal_string_entry(mapping, minor).context("Python minor version not literal")?;
    let block = versions
        .split_once(&format!("    \"{version}\": {{"))
        .and_then(|(_, value)| value.split_once("\n    },"))
        .context("Python runtime metadata missing")?
        .0;
    let digest = literal_string_entry(block, triple).context("Python host runtime hash missing")?;
    ensure!(sha256(digest), "invalid Python runtime hash");
    Ok(digest.to_owned())
}

fn literal_string_entry<'a>(metadata: &'a str, key: &str) -> Option<&'a str> {
    metadata
        .lines()
        .filter_map(|line| line.trim().split_once(':'))
        .find(|(name, _)| name.trim() == format!("\"{key}\""))?
        .1
        .trim()
        .trim_end_matches(',')
        .strip_prefix('"')?
        .strip_suffix('"')
}

fn literal_archive_hashes(metadata: &str, hashes: &mut BTreeSet<String>) -> Result<()> {
    for line in metadata.lines() {
        let Some((key, value)) = line.trim().split_once('=') else {
            continue;
        };
        if key.trim() != "sha256" {
            continue;
        }
        // Computed expressions are not dependency evidence. Exact literals only.
        if let Some(digest) = value
            .trim()
            .trim_end_matches(',')
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
            .filter(|value| sha256(value))
        {
            hashes.insert(digest.to_owned());
        }
    }
    ensure!(
        hashes.len() <= MAX_FILES,
        "too many rules_rust dependency hashes"
    );
    Ok(())
}

fn sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn safe_component(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.+".contains(&b))
}

fn checked_directory(path: &Path, create: bool) -> Result<()> {
    ensure!(path.is_absolute(), "build cache paths must be absolute");
    let mut current = PathBuf::new();
    for component in path.components() {
        ensure!(
            !matches!(component, Component::ParentDir | Component::CurDir),
            "non-canonical cache path"
        );
        current.push(component);
        if create && !current.exists() {
            fs::create_dir(&current)?;
            fs::set_permissions(&current, fs::Permissions::from_mode(0o700))?;
        }
        let metadata = fs::symlink_metadata(&current)?;
        ensure!(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "cache directory must not traverse a symlink"
        );
    }
    Ok(())
}

fn regular_file(path: &Path) -> Result<File> {
    checked_directory(path.parent().context("cache file lacks parent")?, false)?;
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    ensure!(
        file.metadata()?.is_file(),
        "cache input must be a regular file"
    );
    Ok(file)
}

fn read_bounded(path: &Path, maximum: u64) -> Result<Vec<u8>> {
    let mut file = regular_file(path)?;
    ensure!(
        file.metadata()?.len() <= maximum,
        "cache metadata exceeds byte limit"
    );
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(maximum + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= maximum,
        "cache metadata grew beyond byte limit"
    );
    Ok(bytes)
}

fn bounded_entries(path: &Path) -> Result<Vec<fs::DirEntry>> {
    let entries = fs::read_dir(path)?
        .take(MAX_FILES + 1)
        .collect::<std::io::Result<Vec<_>>>()?;
    ensure!(entries.len() <= MAX_FILES, "too many cache entries");
    Ok(entries)
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
