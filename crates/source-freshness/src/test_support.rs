//! Per-test local Git repositories and a deterministic remote CLI fixture.
use crate::RemoteClient;
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    time::Duration,
};

pub(super) fn git(cwd: &Path, args: &[&str]) -> String {
    process::checked(
        "git",
        &args.iter().map(|s| (*s).into()).collect::<Vec<_>>(),
        cwd,
    )
    .unwrap()
}

pub(super) fn fixture() -> (tempfile::TempDir, RemoteClient, PathBuf, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path();
    let remote = base.join("remote.git");
    git(
        base,
        &[
            "init",
            "--bare",
            "--initial-branch=main",
            remote.to_str().unwrap(),
        ],
    );
    let author = base.join("author");
    fs::create_dir(&author).unwrap();
    git(&author, &["init", "--initial-branch=main"]);
    git(&author, &["config", "user.name", "Freshness Fixture"]);
    git(
        &author,
        &["config", "user.email", "fixture@example.invalid"],
    );
    fs::write(author.join("tracked.txt"), "first remote commit").unwrap();
    git(&author, &["add", "tracked.txt"]);
    git(&author, &["commit", "-m", "first"]);
    git(
        &author,
        &["remote", "add", "origin", remote.to_str().unwrap()],
    );
    git(&author, &["push", "origin", "main"]);
    let checkout = base.join("checkout");
    git(
        base,
        &[
            "clone",
            remote.to_str().unwrap(),
            checkout.to_str().unwrap(),
        ],
    );
    git(
        &checkout,
        &[
            "remote",
            "set-url",
            "origin",
            "https://github.com/coolplayagent/freshness-fixture.git",
        ],
    );
    git(
        &checkout,
        &[
            "config",
            &format!("url.{}.insteadOf", remote.display()),
            "https://github.com/coolplayagent/freshness-fixture.git",
        ],
    );
    git(&checkout, &["checkout", "-b", "feature/local-work"]);
    fs::write(checkout.join("tracked.txt"), "uncommitted user work").unwrap();
    fs::write(base.join("default-branch"), "main").unwrap();
    fs::write(
        base.join("release.json"),
        r#"{"id":42,"tag_name":"v1","draft":false,"prerelease":false}"#,
    )
    .unwrap();
    let gh = base.join("fake-gh");
    fs::write(&gh,r#"#!/bin/sh
set -eu
base=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
if [ -e "$base/offline" ]; then printf 'network unavailable\n' >&2; exit 1; fi
branch=$(cat "$base/default-branch")
case "$1 $2" in
  'repo view') printf '{"nameWithOwner":"coolplayagent/freshness-fixture","defaultBranchRef":{"name":"%s"}}\n' "$branch" ;;
  'api repos/coolplayagent/freshness-fixture/releases/latest')
    if [ -e "$base/no-release" ]; then printf '{"status":"404","message":"Not Found"}\n'; printf 'gh: Not Found (HTTP 404)\n' >&2; exit 1; fi
    cat "$base/release.json" ;;
  api*) sha=$(git --git-dir="$base/remote.git" rev-parse "refs/heads/$branch"); printf '{"sha":"%s"}\n' "$sha" ;;
  *) exit 9 ;;
esac
"#).unwrap();
    fs::set_permissions(&gh, fs::Permissions::from_mode(0o700)).unwrap();
    let client = RemoteClient {
        git: "git".into(),
        gh,
        timeout: Duration::from_secs(10),
    };
    (temp, client, checkout, author)
}

/// File-backed replies keep malformed protocol data separate from shell syntax.
pub(super) fn reply_client(root: &Path) -> RemoteClient {
    let gh = root.join("reply-gh");
    fs::write(
        &gh,
        r#"#!/bin/sh
set -eu
base=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
case "$1" in
  repo) cat "$base/repository.json" ;;
  api)
    cat "$base/response.json"
    if [ -f "$base/stderr" ]; then cat "$base/stderr" >&2; exit 1; fi ;;
  *) exit 9 ;;
esac
"#,
    )
    .unwrap();
    fs::set_permissions(&gh, fs::Permissions::from_mode(0o700)).unwrap();
    storage::write(
        &root.join("repository.json"),
        &serde_json::json!({
            "nameWithOwner":"coolplayagent/freshness-fixture",
            "defaultBranchRef":{"name":"main"}
        }),
    )
    .unwrap();
    RemoteClient {
        git: "unused-git".into(),
        gh,
        timeout: Duration::from_secs(2),
    }
}
