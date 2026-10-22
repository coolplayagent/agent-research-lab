use super::*;
use collaboration_model::{EvidencePins, KnowledgeWriteInput, OutcomeInput};
fn fixture() -> (tempfile::TempDir, Target, PathBuf, KnowledgeWrite, Outcome) {
    let temp = tempfile::tempdir().unwrap();
    let target = Target {
        kind: contracts::KnowledgeBackend::Directory,
        path: temp.path().to_path_buf(),
        content_root: "documents".into(),
        enabled: true,
        access: contracts::KnowledgeAccess::ReadWrite,
        reference: None,
        remote: None,
        credential_env: vec![],
    };
    fs::create_dir(temp.path().join("documents")).unwrap();
    let state = temp.path().join("service-state");
    fs::create_dir(&state).unwrap();
    let input = OutcomeInput {
        id: "outcome".into(),
        topic_id: "topic".into(),
        goal_id: "goal".into(),
        kind: contracts::OutcomeKind::Artifact,
        title: "Artifact".into(),
        summary: "Summary".into(),
        conditions: "Condition".into(),
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
        id: "write".into(),
        outcome_id: outcome.input.id.clone(),
        outcome_digest: outcome.digest.clone(),
        acceptance_id: "accepted".into(),
        service_id: "knowledge".into(),
        service_digest: "a".repeat(64),
        repository: "external".into(),
        expected_revision: inspect(&target, None, None).unwrap().0,
        max_seconds: 60,
    };
    let write = KnowledgeWrite {
        request_digest: storage::digest(&serde_json::to_vec(&input).unwrap()),
        input,
        created_ms: 1,
        deadline_ms: collaboration_model::now_ms() + 60000,
        result: None,
    };
    (temp, target, state, write, outcome)
}
#[test]
fn publication_has_a_durable_idempotent_receipt() {
    let (_temp, mut target, state, write, outcome) = fixture();
    target.path = fs::canonicalize(&target.path).unwrap();
    let state = fs::canonicalize(state).unwrap();
    let applied = publish(&target, &state, &write, &outcome).unwrap();
    assert_eq!(applied.state, KnowledgeWriteState::Applied);
    assert_eq!(
        lookup(&target, &state, &write).unwrap().unwrap().revision,
        applied.revision
    );
    assert_eq!(
        publish(&target, &state, &write, &outcome).unwrap().revision,
        applied.revision
    );
}
#[test]
fn interrupted_publication_recovers_from_journal_without_remote_writes() {
    let (temp, target, state, write, outcome) = fixture();
    let journal = Journal {
        request: write.clone(),
        path: "accepted-outcomes/outcome.json".into(),
        before: None,
        after: outcome.clone(),
    };
    storage::write(&state.join("write.journal.json"), &journal).unwrap();
    assert!(lookup(&target, &state, &write).unwrap().is_none());
    // Simulate interruption between the artifact rename and publication receipt rename.
    storage::write(&temp.path().join("documents").join(&journal.path), &outcome).unwrap();
    let revision = inspect(&target, None, None).unwrap().0;
    let recovered = lookup(&target, &state, &write).unwrap().unwrap();
    assert_eq!(recovered.state, KnowledgeWriteState::Applied);
    assert_eq!(inspect(&target, None, None).unwrap().0, revision);
    assert!(
        !temp
            .path()
            .join("documents/.outcomehub/write.json")
            .exists()
    );
    fs::write(
        temp.path().join("documents").join(&journal.path),
        "conflicting content",
    )
    .unwrap();
    assert!(lookup(&target, &state, &write).unwrap().is_none());
}
