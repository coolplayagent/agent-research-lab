use crate::test_support::{fixture, git};
use std::fs;

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
fn remote_binding_rejects_malformed_identity_branch_and_commit() {
    use crate::{RemoteSnapshot, test_support::reply_client};
    use serde_json::json;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let client = reply_client(root);
    let snapshot = RemoteSnapshot {
        repository: "coolplayagent/freshness-fixture".into(),
        default_branch: "main".into(),
        commit: "a".repeat(40),
        checked_at: "2026-10-09T00:00:00Z".into(),
    };
    storage::write(&root.join("response.json"), &json!({"sha":snapshot.commit})).unwrap();
    client.verify_remote_binding(&snapshot).unwrap();
    for commit in [
        json!("A".repeat(40)),
        json!("a".repeat(39)),
        json!(null),
        json!(false),
    ] {
        storage::write(&root.join("response.json"), &json!({"sha":commit})).unwrap();
        assert!(client.verify_remote_binding(&snapshot).is_err());
    }
    storage::write(&root.join("response.json"), &json!({"sha":snapshot.commit})).unwrap();
    for branch in ["../main", "--upload-pack=other", "main.lock", "main\nnext"] {
        storage::write(
            &root.join("repository.json"),
            &json!({
                "nameWithOwner":snapshot.repository,"defaultBranchRef":{"name":branch}
            }),
        )
        .unwrap();
        assert!(client.verify_remote_binding(&snapshot).is_err());
    }
    storage::write(
        &root.join("repository.json"),
        &json!({
            "nameWithOwner":"other/repo","defaultBranchRef":{"name":"main"}
        }),
    )
    .unwrap();
    assert!(client.verify_remote_binding(&snapshot).is_err());
}
