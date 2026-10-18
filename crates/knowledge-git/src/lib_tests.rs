use super::*;
use collaboration_model::{EvidencePins, KnowledgeWriteInput, OutcomeInput};
use contracts::OutcomeKind;

fn setup() -> (tempfile::TempDir, BTreeMap<String, GitTarget>, String) {
    let root = tempfile::tempdir().unwrap();
    let repository = root.path().join("external");
    fs::create_dir(&repository).unwrap();
    let target = GitTarget {
        path: repository,
        reference: "refs/heads/incoming".into(),
    };
    let git = Git {
        target: &target,
        run: tempfile::tempdir_in(root.path()).unwrap(),
        deadline: Instant::now() + Duration::from_secs(30),
        prefix: PREFIX.into(),
        remote: None,
        credential_env: vec![],
    };
    git.call(&["init", "-b", "main"]).unwrap();
    let tree = git
        .call(&["hash-object", "-t", "tree", "-w", "/dev/null"])
        .unwrap();
    let head = git
        .call(&["commit-tree", &tree, "-m", "Initial knowledge"])
        .unwrap();
    git.call(&["update-ref", "refs/heads/main", &head]).unwrap();
    git.call(&["update-ref", &target.reference, &head]).unwrap();
    let repositories = BTreeMap::from([("external".into(), target.clone())]);
    (root, repositories, head)
}
fn proposal(head: &str) -> (KnowledgeWrite, Outcome) {
    let input = OutcomeInput {
        id: "finding".into(),
        topic_id: "topic".into(),
        goal_id: "goal".into(),
        kind: OutcomeKind::Finding,
        title: "Validated finding".into(),
        summary: "Evidence summary".into(),
        conditions: "Pinned test".into(),
        evidence: vec![],
        references: vec![],
        supersedes: None,
        attempt: None,
        pins: EvidencePins::default(),
        knowledge: None,
    };
    let outcome = Outcome {
        digest: storage::digest(&serde_json::to_vec(&("operator", &input)).unwrap()),
        input,
        author_id: "operator".into(),
        created_ms: 1,
        sequence: 1,
    };
    let input = KnowledgeWriteInput {
        id: "publication".into(),
        outcome_id: "finding".into(),
        outcome_digest: outcome.digest.clone(),
        acceptance_id: "acceptance".into(),
        service_id: "knowledge".into(),
        service_digest: "a".repeat(64),
        repository: "external".into(),
        expected_revision: head.into(),
        max_seconds: 60,
    };
    let write = KnowledgeWrite {
        request_digest: storage::digest(&serde_json::to_vec(&input).unwrap()),
        input,
        created_ms: collaboration_model::now_ms(),
        deadline_ms: collaboration_model::now_ms() + 60000,
        result: None,
    };
    (write, outcome)
}
#[test]
fn external_publication_is_cas_idempotent_and_reconciles_after_a_lost_receipt() {
    let (root, repositories, head) = setup();
    let state = root.path().join("publisher");
    fs::create_dir(&state).unwrap();
    let (write, outcome) = proposal(&head);
    let request = OutcomeServiceRequest::Write {
        write: write.clone(),
        outcome: Box::new(outcome.clone()),
    };
    let published = dispatch("knowledge", &repositories, &state, request.clone()).unwrap();
    assert_eq!(published["state"], KnowledgeWriteState::Applied.as_str());
    assert_ne!(published["revision"], head);
    assert_eq!(
        dispatch("knowledge", &repositories, &state, request).unwrap(),
        published
    );
    let target = &repositories["external"];
    let git = Git {
        target,
        run: tempfile::tempdir_in(root.path()).unwrap(),
        deadline: Instant::now() + Duration::from_secs(30),
        prefix: PREFIX.into(),
        remote: None,
        credential_env: vec![],
    };
    assert_eq!(git.call(&["rev-parse", "HEAD"]).unwrap(), head);
    let raw = git
        .call(&[
            "show",
            &format!("{}:accepted-outcomes/finding.json", target.reference),
        ])
        .unwrap();
    let stored: Outcome = serde_json::from_str(&raw).unwrap();
    assert_eq!(stored.digest, outcome.digest);
    let path = state.join("receipts/publication.json");
    let mut record: Receipt = storage::read(&path).unwrap();
    record.result = result(
        &write,
        KnowledgeWriteState::Unknown,
        String::new(),
        String::new(),
        "Interrupted after Git commit",
    );
    storage::write(&path, &record).unwrap();
    let recovered = dispatch(
        "knowledge",
        &repositories,
        &state,
        OutcomeServiceRequest::LookupWrite {
            id: write.input.id.clone(),
            request_digest: write.request_digest.clone(),
        },
    )
    .unwrap();
    assert_eq!(recovered, published);
    let (mut stale, outcome) = proposal(&head);
    stale.input.id = "stale".into();
    stale.request_digest = storage::digest(&serde_json::to_vec(&stale.input).unwrap());
    let rejected = dispatch(
        "knowledge",
        &repositories,
        &state,
        OutcomeServiceRequest::Write {
            write: stale,
            outcome: Box::new(outcome),
        },
    )
    .unwrap();
    assert_eq!(rejected["state"], KnowledgeWriteState::Failed.as_str());
    assert_eq!(git.head().unwrap(), published["revision"].as_str().unwrap());
    assert!(
        dispatch(
            "knowledge",
            &repositories,
            &state,
            OutcomeServiceRequest::LookupWrite {
                id: "publication".into(),
                request_digest: "forged".into()
            }
        )
        .is_err()
    );
}
#[test]
fn checked_out_branches_and_path_injection_cannot_be_written() {
    let (root, mut repositories, head) = setup();
    let state = root.path().join("publisher");
    fs::create_dir(&state).unwrap();
    repositories.get_mut("external").unwrap().reference = "refs/heads/main".into();
    let (write, outcome) = proposal(&head);
    let result = dispatch(
        "knowledge",
        &repositories,
        &state,
        OutcomeServiceRequest::Write {
            write: write.clone(),
            outcome: Box::new(outcome.clone()),
        },
    )
    .unwrap();
    assert_eq!(result["state"], KnowledgeWriteState::Unknown.as_str());
    assert!(
        !repositories["external"]
            .path
            .join("accepted-outcomes")
            .exists()
    );
    let mut invalid = write;
    invalid.input.id = "../escape".into();
    assert!(
        dispatch(
            "knowledge",
            &repositories,
            &state,
            OutcomeServiceRequest::Write {
                write: invalid,
                outcome: Box::new(outcome)
            }
        )
        .is_err()
    );
}
