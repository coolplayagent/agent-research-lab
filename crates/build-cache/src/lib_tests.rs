use super::*;
use std::os::unix::fs::{MetadataExt, symlink};

fn fixture() -> (
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
fn resuming_verifies_existing_files_and_keeps_them_in_the_total_budget() {
    let (_temp, worktree, logs, cargo, cache, _) = fixture();
    let mut seed = Seeder {
        started: Instant::now(),
        report: SeedReport::default(),
    };
    let archive = cargo.join("registry/cache/index.crates.io-fixture/example-1.0.0.crate");
    let destination =
        logs.join("home/.cargo/registry/cache/index.crates.io-fixture/example-1.0.0.crate");
    let checksum = storage::digest(b"public archive");
    seed.copy(&archive, &destination, Some(&checksum), false)
        .unwrap();
    let inode = fs::metadata(&destination).unwrap().ino();
    seed.started = Instant::now() - Duration::from_secs(56);
    assert!(deferred(
        &seed
            .copy(&archive, &destination, Some(&checksum), false)
            .unwrap_err()
    ));
    let report = prepare_from(&worktree, &logs, Some(&cargo), Some(&cache)).unwrap();
    assert_eq!(report.reused_files, 1);
    assert_eq!(report.retained_files, 3);
    assert_eq!(fs::metadata(&destination).unwrap().ino(), inode);
    seed.started = Instant::now();
    seed.report.retained_bytes = MAX_BYTES;
    assert!(
        seed.copy(&archive, &destination, Some(&checksum), false)
            .unwrap_err()
            .to_string()
            .contains("budget")
    );
    seed.report.retained_bytes = 0;
    seed.report.retained_files = MAX_FILES;
    assert!(
        seed.copy(&archive, &destination, Some(&checksum), false)
            .unwrap_err()
            .to_string()
            .contains("budget")
    );
    fs::write(&destination, "damaged private cache").unwrap();
    let report = prepare_from(&worktree, &logs, Some(&cargo), Some(&cache)).unwrap();
    assert!(report.copied_files > 0);
    assert_eq!(fs::read(&destination).unwrap(), b"public archive");
}

#[test]
fn registry_integrity_decoding_rejects_invalid_padding() {
    assert_eq!(
        decode_sha256("47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU=").unwrap(),
        storage::digest(b"")
    );
    assert!(decode_sha256("47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFV=").is_err());
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
fn python_metadata_selects_only_the_default_exact_host_runtime() {
    let module = "python.defaults(\n python_version = \"3.11\",\n)";
    let expected = "a".repeat(64);
    let other = "b".repeat(64);
    let versions = format!(
        r#"TOOL_VERSIONS = {{
    "3.11.14": {{
        "sha256": {{
            "x86_64-unknown-linux-gnu": "{expected}",
            "aarch64-unknown-linux-gnu": "{other}",
        }},
    }},
    "3.12.1": {{
        "sha256": {{
            "x86_64-unknown-linux-gnu": "{other}",
        }},
    }},
}}
MINOR_MAPPING = {{
    "3.11": "3.11.14",
    "3.12": "3.12.1",
}}
"#
    );
    assert_eq!(
        python_default_hash(module, &versions, "x86_64-unknown-linux-gnu").unwrap(),
        expected
    );
    assert!(python_default_hash(module, &versions, "unknown-platform").is_err());
    assert!(
        python_default_hash(
            "python.defaults(python_version = dynamic())",
            &versions,
            "x86_64-unknown-linux-gnu"
        )
        .is_err()
    );
}

#[test]
fn bootstrap_metadata_only_imports_literal_archive_hashes() {
    let expected = "a".repeat(64);
    let ignored = "b".repeat(64);
    let metadata = format!(
        "sha256 = \"{expected}\",\n# sha256 = \"{ignored}\",\nname = \"{ignored}\",\nsha256 = lookup(\"{ignored}\"),\n"
    );
    let mut hashes = BTreeSet::new();
    literal_archive_hashes(&metadata, &mut hashes).unwrap();
    assert_eq!(hashes, BTreeSet::from([expected]));
    let patch = serde_json::json!({"patches": {"fix.patch": "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="}});
    json_hashes(&patch, &mut hashes).unwrap();
    assert!(hashes.contains(&"0".repeat(64)));
}

#[test]
#[ignore = "requires installed Rust/Bazel caches and real bubblewrap; LAB_BUILD_CACHE_PROOF selects output"]
fn actual_seeded_build_in_agent_namespace() {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    assert!(source.join("MODULE.bazel").is_file());
    assert!(source.join("app/Cargo.toml").is_file());
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
