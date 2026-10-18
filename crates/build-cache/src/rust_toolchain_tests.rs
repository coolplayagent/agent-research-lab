use super::*;
use crate::metadata::json_hashes;

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
fn rust_metadata_selects_pinned_host_tools_without_executing_starlark() {
    let temp = tempfile::tempdir().unwrap();
    let triple = format!("{}-unknown-linux-gnu", std::env::consts::ARCH);
    let default = "a".repeat(64);
    let pinned = "b".repeat(64);
    let formatter = "c".repeat(64);
    let bootstrap = "d".repeat(64);
    let ignored = "e".repeat(64);
    let known = format!(
        "\"rustc-1.80.0-{triple}.tar.xz\": \"{default}\",\n\"cargo-1.81.0-{triple}.tar.xz\": \"{pinned}\",\n\"2024-08-01/rustfmt-nightly-{triple}.tar.xz\": \"{formatter}\",\n\"rustc-1.80.0-other-platform.tar.xz\": \"{ignored}\",\n\"rustc-1.99.0-{triple}.tar.xz\": \"{ignored}\",\n"
    );
    let literals = format!(
        "fail(\"metadata must never execute\")\nsha256 = \"{bootstrap}\",\nsha256 = compute(\"{ignored}\"),\n"
    );
    let (cas, metadata) = crate::test_support::verified_tar(
        temp.path(),
        &[
            ("rust/private/repository_utils.bzl", &literals),
            (
                "cargo/3rdparty/crates/crates.bzl",
                "# no extra dependencies\n",
            ),
            (
                "rust/private/common.bzl",
                "DEFAULT_RUST_VERSION = \"1.80.0\"\nDEFAULT_NIGHTLY_ISO_DATE = \"2024-08-01\"\n",
            ),
            ("rust/private/known_shas.bzl", &known),
        ],
    );
    let worktree = temp.path().join("worktree");
    crate::files::checked_directory(&worktree, true).unwrap();
    native_effects::fs::write(
        worktree.join("rust-toolchain.toml"),
        "[toolchain]\nchannel = '1.81.0'\n",
    )
    .unwrap();
    native_effects::fs::write(
        worktree.join("MODULE.bazel"),
        "rust.toolchain(versions = [\"1.81.0\"])\n",
    )
    .unwrap();
    let mut hashes = BTreeSet::new();
    rust_toolchain_hashes(&worktree, &cas, &metadata, &mut hashes).unwrap();
    assert_eq!(
        hashes,
        BTreeSet::from([
            default.clone(),
            pinned,
            formatter.clone(),
            bootstrap.clone()
        ])
    );
    native_effects::fs::write(
        worktree.join("MODULE.bazel"),
        "rust.toolchain(versions = dynamic())\n",
    )
    .unwrap();
    hashes.clear();
    rust_toolchain_hashes(&worktree, &cas, &metadata, &mut hashes).unwrap();
    assert_eq!(hashes, BTreeSet::from([default, formatter, bootstrap]));
}
