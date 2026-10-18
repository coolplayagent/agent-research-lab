use super::*;
use std::os::unix::fs::PermissionsExt;
fn git(cwd: &Path, args: &[&str]) -> String {
    process::checked(
        "git",
        &args.iter().map(|s| (*s).into()).collect::<Vec<_>>(),
        cwd,
    )
    .unwrap()
}
fn fixture() -> (tempfile::TempDir, RemoteClient, PathBuf, PathBuf) {
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
#[test]
fn fetches_remote_default_without_touching_feature_head_or_user_changes() {
    let (temp, client, checkout, author) = fixture();
    let local_head = git(&checkout, &["rev-parse", "HEAD"]);
    let local_diff = git(&checkout, &["diff"]);
    let first = client.remote_snapshot(&checkout).unwrap();
    assert_eq!(first.commit, local_head);
    assert_eq!(first.default_branch, "main");
    fs::write(author.join("tracked.txt"), "latest remote work").unwrap();
    git(&author, &["commit", "-am", "advance remote default"]);
    git(&author, &["push", "origin", "main"]);
    assert!(client.verify_fresh(&checkout, &first).is_err());
    assert!(client.verify_remote_binding(&first).is_err());
    let latest = client.remote_snapshot(&checkout).unwrap();
    assert_eq!(latest.commit, git(&author, &["rev-parse", "HEAD"]));
    assert_ne!(latest.commit, first.commit);
    assert_eq!(
        git(&checkout, &["branch", "--show-current"]),
        "feature/local-work"
    );
    assert_eq!(git(&checkout, &["rev-parse", "HEAD"]), local_head);
    assert_eq!(git(&checkout, &["diff"]), local_diff);
    fs::write(temp.path().join("offline"), "").unwrap();
    assert!(client.remote_snapshot(&checkout).is_err());
    assert!(client.verify_remote_binding(&latest).is_err());
}
#[test]
fn default_branch_switch_invalidates_same_commit_binding() {
    let (temp, client, checkout, author) = fixture();
    let first = client.remote_snapshot(&checkout).unwrap();
    git(&author, &["push", "origin", "HEAD:refs/heads/stable"]);
    fs::write(temp.path().join("default-branch"), "stable").unwrap();
    assert!(client.verify_remote_binding(&first).is_err());
    let current = client.remote_snapshot(&checkout).unwrap();
    assert_eq!(current.commit, first.commit);
    assert_eq!(current.default_branch, "stable");
}
#[test]
fn skill_hash_and_upstream_release_or_source_must_all_be_current() {
    let (temp, client, _checkout, author) = fixture();
    let installed = temp.path().join("skill");
    fs::create_dir(&installed).unwrap();
    fs::write(installed.join("SKILL.md"), "Use the tested CLI.").unwrap();
    let runtime = installed.join("runtime");
    fs::write(&runtime, b"runtime-v1").unwrap();
    let mut manifest = SkillsManifest {
        schema_version: 1,
        skills: vec![SkillSnapshot {
            name: "workflow-cli".into(),
            repository: "coolplayagent/freshness-fixture".into(),
            installed_path: installed.clone(),
            runtime_path: runtime.clone(),
            tree_sha256: skill_tree_digest(&installed).unwrap(),
            runtime_sha256: hex(&Sha256::digest(b"runtime-v1")),
            release_tag: Some("v1".into()),
            release_id: Some(42),
            source_commit: None,
            source_qualitygate: None,
            checked_at: Utc::now().to_rfc3339(),
        }],
    };
    let path = temp.path().join("skills.json");
    storage::write(&path, &manifest).unwrap();
    client.verify_skills(&path).unwrap();
    verify_installed_bytes(&manifest).unwrap();
    fs::write(installed.join("SKILL.md"), "modified instructions").unwrap();
    assert!(client.verify_skills(&path).is_err());
    assert!(
        verify_installed_bytes(&manifest).is_err(),
        "a mid-attempt installation must invalidate old observations without a network call"
    );
    fs::write(installed.join("SKILL.md"), "Use the tested CLI.").unwrap();
    fs::write(
        temp.path().join("release.json"),
        r#"{"id":43,"tag_name":"v2","draft":false,"prerelease":false}"#,
    )
    .unwrap();
    assert!(client.verify_skills(&path).is_err());
    manifest.skills[0].release_tag = None;
    manifest.skills[0].release_id = None;
    manifest.skills[0].source_commit = Some(git(&author, &["rev-parse", "HEAD"]));
    storage::write(&path, &manifest).unwrap();
    assert!(
        client.verify_skills(&path).is_err(),
        "an unverified source fallback cannot ignore an existing release"
    );
    let proof_path = temp.path().join("source-gate.json");
    let mut proof = serde_json::json!({
        "profile":"full","scope":"delivery",
        "gate":{"complete":true,"decision":"pass"},
        "plan":{"pending_delivery_checks":[]},
        "selection":{"empty_delivery":false},
        "snapshot":{"mode":"diff","head":manifest.skills[0].source_commit}
    });
    storage::write(&proof_path, &proof).unwrap();
    manifest.skills[0].source_qualitygate = Some(SourceQualitygate {
        path: proof_path.clone(),
        sha256: storage::digest(&fs::read(&proof_path).unwrap()),
    });
    storage::write(&path, &manifest).unwrap();
    client.verify_skills(&path).unwrap();
    let retained_proof = fs::read(&proof_path).unwrap();
    fs::OpenOptions::new()
        .write(true)
        .open(&proof_path)
        .unwrap()
        .set_len(16 * 1024 * 1024 + 1)
        .unwrap();
    assert!(
        verify_source_qualitygate(&manifest.skills[0])
            .unwrap_err()
            .to_string()
            .contains("16 MiB"),
        "oversized reports must be rejected before parsing"
    );
    fs::write(&proof_path, &retained_proof).unwrap();
    let replacement = temp.path().join("replacement-source-gate.json");
    fs::rename(&proof_path, &replacement).unwrap();
    std::os::unix::fs::symlink(&replacement, &proof_path).unwrap();
    assert!(verify_source_qualitygate(&manifest.skills[0]).is_err());
    fs::remove_file(&proof_path).unwrap();
    fs::rename(&replacement, &proof_path).unwrap();
    verify_source_qualitygate(&manifest.skills[0]).unwrap();
    proof["snapshot"]["head"] = "0".repeat(40).into();
    storage::write(&proof_path, &proof).unwrap();
    assert!(
        client.verify_skills(&path).is_err(),
        "changed proof must fail"
    );
    manifest.skills[0]
        .source_qualitygate
        .as_mut()
        .unwrap()
        .sha256 = storage::digest(&fs::read(&proof_path).unwrap());
    storage::write(&path, &manifest).unwrap();
    assert!(
        client.verify_skills(&path).is_err(),
        "a passing gate on another source is insufficient"
    );
    manifest.skills[0].source_qualitygate = None;
    storage::write(&path, &manifest).unwrap();
    fs::write(temp.path().join("no-release"), "").unwrap();
    client.verify_skills(&path).unwrap();
    fs::write(author.join("tracked.txt"), "new source skill version").unwrap();
    git(&author, &["commit", "-am", "source advances"]);
    git(&author, &["push", "origin", "main"]);
    assert!(client.verify_skills(&path).is_err());
}
#[test]
fn tree_digest_matches_public_contract_and_rejects_symlinks() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("b.txt"), b"B").unwrap();
    fs::write(temp.path().join("a.txt"), b"A").unwrap();
    fs::create_dir(temp.path().join("folder")).unwrap();
    fs::write(temp.path().join("folder/a.txt"), b"D").unwrap();
    fs::write(temp.path().join("folder.txt"), b"C").unwrap();
    let mut expected = Sha256::new();
    for (path, content) in [
        ("a.txt", b"A"),
        ("b.txt", b"B"),
        ("folder.txt", b"C"),
        ("folder/a.txt", b"D"),
    ] {
        expected.update(path.as_bytes());
        expected.update([0]);
        expected.update(Sha256::digest(content));
        expected.update(b"\n");
    }
    assert_eq!(
        skill_tree_digest(temp.path()).unwrap(),
        hex(&expected.finalize())
    );
    std::os::unix::fs::symlink(temp.path().join("a.txt"), temp.path().join("alias")).unwrap();
    assert!(skill_tree_digest(temp.path()).is_err());
}
#[test]
fn rejects_untrusted_origins_and_compares_identity_without_check_time() {
    assert!(github_origin("https://github.com/other/repo.git").is_err());
    assert!(github_origin("https://token@github.com/coolplayagent/repo.git").is_err());
    assert_eq!(
        github_origin("git@github.com:stevetdp/superpod.git").unwrap(),
        "stevetdp/superpod"
    );
    let first = RemoteSnapshot {
        repository: "coolplayagent/repo".into(),
        default_branch: "main".into(),
        commit: "a".repeat(40),
        checked_at: Utc::now().to_rfc3339(),
    };
    let mut later = first.clone();
    later.checked_at = "2026-10-09T00:00:00Z".into();
    assert!(first.same_binding(&later));
    assert_ne!(first, later);
}
