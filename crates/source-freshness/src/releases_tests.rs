use crate::test_support::reply_client;
use serde_json::json;
use std::fs;

#[test]
fn missing_release_requires_matching_repository_and_explicit_http_404() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let client = reply_client(root);
    let repository = "coolplayagent/freshness-fixture";
    for status in [json!("404"), json!(404)] {
        storage::write(&root.join("response.json"), &json!({"status":status})).unwrap();
        fs::write(root.join("stderr"), "gh: Not Found (HTTP 404)").unwrap();
        assert_eq!(client.latest_release(repository).unwrap(), None);
    }
    fs::write(root.join("stderr"), "network unavailable").unwrap();
    assert!(client.latest_release(repository).is_err());
    fs::write(root.join("stderr"), "gh: Not Found (HTTP 404)").unwrap();
    for status in [json!(401), json!("unknown"), json!(null)] {
        storage::write(&root.join("response.json"), &json!({"status":status})).unwrap();
        assert!(client.latest_release(repository).is_err());
    }
    storage::write(&root.join("response.json"), &json!({"status":404})).unwrap();
    storage::write(
        &root.join("repository.json"),
        &json!({
            "nameWithOwner":"coolplayagent/different",
            "defaultBranchRef":{"name":"main"}
        }),
    )
    .unwrap();
    assert!(client.latest_release(repository).is_err());
    fs::remove_file(root.join("repository.json")).unwrap();
    assert!(client.latest_release(repository).is_err());
}

#[test]
fn only_complete_stable_release_identity_is_accepted() {
    let temp = tempfile::tempdir().unwrap();
    let client = reply_client(temp.path());
    let valid = json!({"id":42,"tag_name":"v1","draft":false,"prerelease":false});
    storage::write(&temp.path().join("response.json"), &valid).unwrap();
    assert_eq!(
        client
            .latest_release("coolplayagent/freshness-fixture")
            .unwrap(),
        Some((42, "v1".into()))
    );
    for (key, bad) in [
        ("draft", json!(true)),
        ("draft", json!(null)),
        ("prerelease", json!(true)),
        ("prerelease", json!("false")),
        ("id", json!(0)),
        ("id", json!("42")),
        ("tag_name", json!("")),
    ] {
        let mut changed = valid.clone();
        changed[key] = bad;
        storage::write(&temp.path().join("response.json"), &changed).unwrap();
        assert!(
            client
                .latest_release("coolplayagent/freshness-fixture")
                .is_err(),
            "{key}"
        );
    }
}
