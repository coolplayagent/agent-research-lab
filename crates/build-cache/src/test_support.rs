//! Shared local public-cache fixtures; never use host credentials.
use crate::{REGISTRY, files::checked_directory};
use native_effects::fs;
use std::path::PathBuf;

pub(super) fn fixture() -> (
    tempfile::TempDir,
    PathBuf,
    PathBuf,
    PathBuf,
    PathBuf,
    String,
) {
    let temp = tempfile::tempdir().unwrap();
    let worktree = temp.path().join("worktree");
    let logs = temp.path().join("logs");
    let cargo = temp.path().join("cargo");
    let cache = temp.path().join("cache");
    fs::create_dir_all(&worktree).unwrap();
    let digest = storage::digest(b"public archive");
    fs::write(worktree.join("Cargo.lock"), format!("version = 4\n[[package]]\nname = 'example'\nversion = '1.0.0'\nsource = '{REGISTRY}'\nchecksum = '{digest}'\n")).unwrap();
    fs::write(worktree.join("MODULE.bazel"), "module(name = 'fixture')\n").unwrap();
    let registry = "index.crates.io-fixture";
    let archive = cargo
        .join("registry/cache")
        .join(registry)
        .join("example-1.0.0.crate");
    checked_directory(archive.parent().unwrap(), true).unwrap();
    fs::write(&archive, b"public archive").unwrap();
    let index = cargo
        .join("registry/index")
        .join(registry)
        .join(".cache/ex/am/example");
    checked_directory(index.parent().unwrap(), true).unwrap();
    fs::write(index, b"public sparse index").unwrap();
    fs::write(cargo.join("credentials.toml"), "credential-sentinel").unwrap();
    fs::write(cargo.join("config.toml"), "configuration-sentinel").unwrap();
    fs::write(cargo.join(".package-cache"), "host-lock-sentinel").unwrap();
    let cas = cache
        .join("bazel/_bazel_fixture/cache/repos/v1/content_addressable/sha256")
        .join(&digest);
    checked_directory(&cas, true).unwrap();
    fs::write(cas.join("file"), b"public archive").unwrap();
    fs::write(cas.join("id-ignored"), "unrelated canonical metadata").unwrap();
    (temp, worktree, logs, cargo, cache, digest)
}

pub(super) fn verified_tar(
    root: &std::path::Path,
    files: &[(&str, &str)],
) -> (PathBuf, serde_json::Value) {
    use sha2::{Digest, Sha256};
    let payload = root.join("payload");
    for (name, contents) in files {
        let path = payload.join(name);
        checked_directory(path.parent().unwrap(), true).unwrap();
        fs::write(path, contents).unwrap();
    }
    let archive = root.join("archive.tar");
    let output = process::capture(
        "tar",
        &[
            "-cf".into(),
            archive.display().to_string(),
            "-C".into(),
            payload.display().to_string(),
            ".".into(),
        ],
        root,
        std::time::Duration::from_secs(10),
    )
    .unwrap();
    assert!(output.status.success());
    let bytes = fs::read(&archive).unwrap();
    let digest = Sha256::digest(&bytes);
    let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut encoded = String::new();
    for chunk in digest.chunks(3) {
        let bits = (u32::from(chunk[0]) << 16)
            | (u32::from(chunk.get(1).copied().unwrap_or(0)) << 8)
            | u32::from(chunk.get(2).copied().unwrap_or(0));
        encoded.push(char::from(alphabet[((bits >> 18) & 63) as usize]));
        encoded.push(char::from(alphabet[((bits >> 12) & 63) as usize]));
        encoded.push(if chunk.len() > 1 {
            char::from(alphabet[((bits >> 6) & 63) as usize])
        } else {
            '='
        });
        encoded.push(if chunk.len() > 2 {
            char::from(alphabet[(bits & 63) as usize])
        } else {
            '='
        });
    }
    let cas = root.join("cas");
    let destination = cas.join(storage::digest(&bytes)).join("file");
    checked_directory(destination.parent().unwrap(), true).unwrap();
    fs::rename(archive, destination).unwrap();
    (
        cas,
        serde_json::json!({"integrity":format!("sha256-{encoded}")}),
    )
}
