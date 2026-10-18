use super::*;
use crate::{
    merge_gate::evaluate_merge_gate,
    tests::{evidence, upstream},
};
use contracts::{
    CheckConclusion, CheckState, MergeQueueState, MergeabilityState, PullRequestState,
};
use serde_json::json;

fn response() -> Value {
    json!({"data":{"repository":{
        "defaultBranchRef":{"name":"main","target":{"oid":upstream().commit}},
        "pullRequest":{
            "headRefOid":"a".repeat(40),"baseRefName":"main","baseRefOid":upstream().commit,
            "state":PullRequestState::Open,"isDraft":false,"author":{"login":"author"},
            "mergeable":MergeabilityState::Mergeable,"mergeStateStatus":MergeQueueState::Clean,
            "reviewDecision":null,
            "commits":{"nodes":[{"commit":{"oid":"a".repeat(40),"statusCheckRollup":{"contexts":{
                "nodes":[{"__typename":"CheckRun","status":CheckState::Completed,"conclusion":CheckConclusion::Success}],
                "pageInfo":{"hasNextPage":false}
            }}}}]},
            "reviews":{"nodes":[],"pageInfo":{"hasNextPage":false}},
            "reviewThreads":{"nodes":[],"pageInfo":{"hasNextPage":false}}
        }
    }}})
}

#[test]
fn snapshot_checks_cannot_treat_pending_unknown_or_skipped_runs_as_success() {
    let temp = tempfile::tempdir().unwrap();
    let request = MergeRequest {
        repository: upstream().repository,
        working_directory: temp.path().into(),
        number: 1,
        expected_head: "a".repeat(40),
        evidence: evidence(temp.path()),
    };
    for (node, expected, passes) in [
        (
            json!({"__typename":"CheckRun","status":CheckState::Completed,"conclusion":CheckConclusion::Success}),
            CheckConclusion::Success,
            true,
        ),
        (
            json!({"__typename":"CheckRun","status":CheckState::InProgress,"conclusion":CheckConclusion::Success}),
            CheckConclusion::Pending,
            false,
        ),
        (
            json!({"__typename":"CheckRun","status":CheckState::Completed,"conclusion":CheckConclusion::Skipped}),
            CheckConclusion::Skipped,
            false,
        ),
        (
            json!({"__typename":"CheckRun","status":CheckState::Completed}),
            CheckConclusion::Unknown,
            false,
        ),
        (
            json!({"__typename":"StatusContext","state":CheckConclusion::Success}),
            CheckConclusion::Success,
            true,
        ),
        (
            json!({"__typename":"StatusContext","state":CheckConclusion::Failure}),
            CheckConclusion::Failure,
            false,
        ),
        (
            json!({"__typename":"FutureCheck","state":CheckConclusion::Success}),
            CheckConclusion::Unknown,
            false,
        ),
    ] {
        let mut value = response();
        value["data"]["repository"]["pullRequest"]["commits"]["nodes"][0]["commit"]["statusCheckRollup"]
            ["contexts"]["nodes"] = json!([node]);
        let snapshot = decode_snapshot(&value).unwrap();
        assert_eq!(snapshot.checks, vec![expected.to_string()]);
        assert_eq!(evaluate_merge_gate(&request, &snapshot).is_ok(), passes);
    }
}

#[test]
fn incomplete_remote_pages_and_different_commit_observations_block_merge() {
    let temp = tempfile::tempdir().unwrap();
    let request = MergeRequest {
        repository: upstream().repository,
        working_directory: temp.path().into(),
        number: 1,
        expected_head: "a".repeat(40),
        evidence: evidence(temp.path()),
    };
    for pointer in [
        "/data/repository/pullRequest/commits/nodes/0/commit/statusCheckRollup/contexts",
        "/data/repository/pullRequest/reviews",
        "/data/repository/pullRequest/reviewThreads",
    ] {
        for partial in [
            json!({"nodes":[],"pageInfo":{"hasNextPage":true}}),
            json!({"nodes":[]}),
            json!({"pageInfo":{"hasNextPage":false}}),
        ] {
            let mut value = response();
            *value.pointer_mut(pointer).unwrap() = partial;
            let snapshot = decode_snapshot(&value).unwrap();
            assert!(!snapshot.complete);
            assert!(evaluate_merge_gate(&request, &snapshot).is_err());
        }
    }
    let mut value = response();
    value["data"]["repository"]["pullRequest"]["commits"]["nodes"][0]["commit"]["oid"] =
        json!("b".repeat(40));
    let snapshot = decode_snapshot(&value).unwrap();
    assert!(!snapshot.complete);
    assert!(evaluate_merge_gate(&request, &snapshot).is_err());
    value = response();
    value["errors"] = json!([{"message":"partial response"}]);
    assert!(decode_snapshot(&value).is_err());
    for malformed in [json!(true), json!(1), json!({}), json!("")] {
        value = response();
        value["data"]["repository"]["pullRequest"]["reviewDecision"] = malformed;
        assert!(decode_snapshot(&value).is_err());
    }
    value = response();
    value["data"]["repository"]["pullRequest"]
        .as_object_mut()
        .unwrap()
        .remove("reviewDecision");
    assert!(decode_snapshot(&value).is_err());
}

#[test]
fn pull_request_receipts_preserve_known_states_and_reject_unknown_observations() {
    let mut value =
        json!({"number":1,"url":"https://example.invalid/pull/1","headRefOid":"a".repeat(40)});
    for state in PullRequestState::ALL {
        value["state"] = json!(state);
        let receipt = pr_receipt(&value, true).unwrap();
        assert_eq!(receipt.state, state.as_str());
        assert_eq!(receipt.head, "a".repeat(40));
        assert!(receipt.reused);
    }
    for state in [
        json!("future_state"),
        json!("merged"),
        Value::Null,
        json!(true),
    ] {
        value["state"] = state;
        assert!(pr_receipt(&value, true).is_err());
        assert!(pr_receipt(&value, false).is_err());
    }
    value.as_object_mut().unwrap().remove("state");
    assert!(pr_receipt(&value, false).is_err());
}
