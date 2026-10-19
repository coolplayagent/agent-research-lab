use super::*;
#[test]
fn a_success_claim_without_the_correct_artifact_fails() {
    let work = tempfile::tempdir().unwrap();
    let mut input = serde_json::json!({"protocol_version":1,"request_id":"r","person_id":"p","goal_id":"g","work_id":"w","attempt":1,"objective":"test","acceptance":"test","instruction":"test","messages":[]}).to_string();
    let challenge = Challenge::prepare(work.path(), &mut input).unwrap();
    assert!(
        !input.contains(
            String::from_utf8(challenge.expected.clone())
                .unwrap()
                .trim()
        )
    );
    assert!(challenge.verify(work.path()).is_err());
    std::fs::write(work.path().join("binding-result.txt"), b"success").unwrap();
    assert!(challenge.verify(work.path()).is_err());
    std::fs::copy(
        work.path().join("binding-input.txt"),
        work.path().join("binding-result.txt"),
    )
    .unwrap();
    challenge.verify(work.path()).unwrap();
}
