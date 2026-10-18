use super::*;
use crate::{MAX_BYTES, REGISTRY, test_support::fixture};
use native_effects::fs::{self, File};
use std::{
    os::unix::fs::{MetadataExt, symlink},
    time::Duration,
};

#[test]
fn private_copies_exclude_credentials_locks_and_unrelated_metadata() {
    let (_temp, worktree, logs, cargo, cache, _) = fixture();
    let report = prepare_from(&worktree, &logs, Some(&cargo), Some(&cache)).unwrap();
    assert_eq!(
        (
            report.cargo_archives,
            report.cargo_index_entries,
            report.bazel_archives
        ),
        (1, 1, 1)
    );
    assert!(ready(&worktree, &logs).unwrap());
    let source = cargo.join("registry/cache/index.crates.io-fixture/example-1.0.0.crate");
    let copy = logs.join("home/.cargo/registry/cache/index.crates.io-fixture/example-1.0.0.crate");
    assert_ne!(
        fs::metadata(&source).unwrap().ino(),
        fs::metadata(&copy).unwrap().ino()
    );
    fs::write(&copy, "task mutation").unwrap();
    assert_eq!(fs::read(&source).unwrap(), b"public archive");
    for name in ["credentials.toml", "config.toml", ".package-cache"] {
        assert!(!logs.join("home/.cargo").join(name).exists());
    }
    assert!(
        !logs
            .join("cache/bazel/_bazel_fixture/cache/repos/v1/contents")
            .exists()
    );
    let second = logs.with_file_name("second");
    prepare_from(&worktree, &second, Some(&cargo), Some(&cache)).unwrap();
    assert_eq!(
        fs::read(
            second.join("home/.cargo/registry/cache/index.crates.io-fixture/example-1.0.0.crate")
        )
        .unwrap(),
        b"public archive"
    );
    fs::write(worktree.join("MODULE.bazel"), "changed source inputs").unwrap();
    assert!(!ready(&worktree, &logs).unwrap());
}

#[test]
fn checksum_corruption_is_rejected_before_ready_marker() {
    let (_temp, worktree, logs, cargo, cache, digest) = fixture();
    fs::write(
        cache
            .join("bazel/_bazel_fixture/cache/repos/v1/content_addressable/sha256")
            .join(digest)
            .join("file"),
        "corrupted",
    )
    .unwrap();
    let error = prepare_from(&worktree, &logs, Some(&cargo), Some(&cache)).unwrap_err();
    assert!(error.to_string().contains("checksum mismatch"));
    assert!(!ready(&worktree, &logs).unwrap());
}

#[test]
fn unrelated_host_cas_is_neither_exposed_nor_counted_against_budget() {
    let (_temp, worktree, logs, cargo, cache, _) = fixture();
    let relative = "bazel/_bazel_fixture/cache/repos/v1/content_addressable/sha256";
    let unknown = cache.join(relative).join("0".repeat(64));
    checked_directory(&unknown, true).unwrap();
    File::create(unknown.join("file"))
        .unwrap()
        .set_len(MAX_BYTES + 1)
        .unwrap();
    let report = prepare_from(&worktree, &logs, Some(&cargo), Some(&cache)).unwrap();
    assert_eq!(report.bazel_archives, 1);
    assert_eq!(report.retained_files, 3);
    assert!(
        !logs
            .join("cache")
            .join(relative)
            .join("0".repeat(64))
            .exists()
    );
}

#[test]
fn malformed_lockfiles_return_errors_without_panicking() {
    for contents in [
        "version=4\n".to_string(),
        format!("[[package]]\nsource='{REGISTRY}'\nversion='1'\n"),
        format!("[[package]]\nsource='{REGISTRY}'\nname='example'\nversion='1'\n"),
    ] {
        let (_temp, worktree, logs, cargo, cache, _) = fixture();
        fs::write(worktree.join("Cargo.lock"), contents).unwrap();
        let result =
            std::panic::catch_unwind(|| prepare_from(&worktree, &logs, Some(&cargo), Some(&cache)));
        assert!(result.is_ok(), "malformed host input must not panic");
        assert!(result.unwrap().is_err());
        assert!(!logs.join("build-cache-seed.json").exists());
    }
}

#[test]
fn symlink_destinations_and_source_archives_cannot_escape() {
    let (temp, worktree, logs, cargo, cache, _) = fixture();
    checked_directory(&logs, true).unwrap();
    symlink(&cargo, logs.join("home")).unwrap();
    assert!(prepare_from(&worktree, &logs, Some(&cargo), Some(&cache)).is_err());
    fs::remove_file(logs.join("home")).unwrap();
    let archive = cargo.join("registry/cache/index.crates.io-fixture/example-1.0.0.crate");
    fs::remove_file(&archive).unwrap();
    let outside = temp.path().join("private");
    fs::write(&outside, "private sentinel").unwrap();
    symlink(&outside, &archive).unwrap();
    assert!(prepare_from(&worktree, &logs, Some(&cargo), Some(&cache)).is_err());
    assert_eq!(fs::read_to_string(outside).unwrap(), "private sentinel");
}

#[test]
fn expired_budget_never_creates_a_ready_marker() {
    let (_temp, worktree, logs, cargo, cache, _) = fixture();
    let _deadline = process::deadline_scope(Duration::ZERO);
    assert!(prepare_from(&worktree, &logs, Some(&cargo), Some(&cache)).is_err());
    assert!(!ready(&worktree, &logs).unwrap());
}

#[test]
#[ignore = "requires installed Rust/Bazel caches and real bubblewrap; LAB_BUILD_CACHE_PROOF selects output"]
fn actual_seeded_build_in_agent_namespace() {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    assert!(source.join("MODULE.bazel").is_file());
    assert!(source.join("apps/cli/Cargo.toml").is_file());
    let proof =
        PathBuf::from(std::env::var("LAB_BUILD_CACHE_PROOF").expect("LAB_BUILD_CACHE_PROOF"));
    assert!(!proof.exists(), "proof directory must be new");
    let worktree = proof.join("worktrees/build");
    let logs = proof.join("runs/build");
    checked_directory(&worktree, true).unwrap();
    let files = std::process::Command::new("git")
        .args(["ls-files", "-z"])
        .current_dir(&source)
        .output()
        .unwrap();
    assert!(files.status.success());
    for relative in files.stdout.split(|b| *b == 0).filter(|b| !b.is_empty()) {
        let relative = std::str::from_utf8(relative).unwrap();
        let target = worktree.join(relative);
        checked_directory(target.parent().unwrap(), true).unwrap();
        std::fs::copy(source.join(relative), target).unwrap();
    }
    let report = prepare(&worktree, &logs).unwrap();
    eprintln!(
        "cache preparation: {} files, {} bytes, {} ms; missing {} Cargo archives, {} Bazel hashes",
        report.retained_files,
        report.retained_bytes,
        report.elapsed_millis,
        report.missing_cargo_archives.len(),
        report.missing_bazel_hashes.len()
    );
    assert!(report.cargo_archives > 0 && report.bazel_archives > 0 && report.bazel_binaries > 0);
    let script = "set -eu; cargo clippy --offline --locked --workspace --all-targets -- -D warnings; bazel --batch build --lockfile_mode=error --repository_disable_download //:outcome-hub-cli //:package";
    let (program, args) = isolation::wrap_agent(
        "/bin/bash",
        &["-c".into(), script.into()],
        &proof,
        &worktree,
        &logs,
        true,
    )
    .unwrap();
    let output = process::capture(&program, &args, &worktree, Duration::from_secs(600)).unwrap();
    fs::write(proof.join("stdout.log"), &output.stdout).unwrap();
    fs::write(proof.join("stderr.log"), &output.stderr).unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn readiness_binds_every_declared_input_and_rejects_unknown_marker_schema() {
    let (_temp, worktree, logs, _cargo, _cache, _) = fixture();
    for name in [
        "Cargo.lock",
        "MODULE.bazel",
        "MODULE.bazel.lock",
        ".bazelversion",
        "rust-toolchain.toml",
    ] {
        let path = worktree.join(name);
        fs::write(&path, "frozen input").unwrap();
        prepare_from(&worktree, &logs, None, None).unwrap();
        assert!(ready(&worktree, &logs).unwrap());
        fs::write(&path, "changed input").unwrap();
        assert!(!ready(&worktree, &logs).unwrap(), "{name}");
    }
    let mut report = prepare_from(&worktree, &logs, None, None).unwrap();
    report.schema_version = 2;
    storage::write(&logs.join("build-cache-seed.json"), &report).unwrap();
    assert!(!ready(&worktree, &logs).unwrap());
}
