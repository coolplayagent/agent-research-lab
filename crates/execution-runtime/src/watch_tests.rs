use super::*;
#[test]
fn repaired_refresh_clears_current_failure_and_preserves_last_success_on_next_failure() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("seed-status.json");
    for failure in 1..=2 {
        assert!(seed_state(&path, || anyhow::bail!("installed input changed")).is_err());
        let state: Value = storage::read(&path).unwrap();
        assert_eq!(state["consecutive_failures"], failure);
        assert_eq!(
            state["next_attempt_at"].as_i64().unwrap() - state["last_attempt_at"].as_i64().unwrap(),
            retry_delay(failure)
        );
        assert!(state["last_success_at"].is_null());
    }
    let result = json!({"queued":["fresh-evidence-task"]});
    assert_eq!(seed_state(&path, || Ok(result.clone())).unwrap(), result);
    let success: Value = storage::read(&path).unwrap();
    assert!(success["error"].is_null());
    assert_eq!(success["consecutive_failures"], 0);
    assert_eq!(success["queued"], result["queued"]);
    assert!(seed_state(&path, || anyhow::bail!("next refresh failed")).is_err());
    let failure: Value = storage::read(&path).unwrap();
    assert_eq!(failure["last_success_at"], success["last_success_at"]);
    assert_eq!(failure["consecutive_failures"], 1);
    assert_eq!(failure["error"], "next refresh failed");
}
#[test]
fn refresh_retry_is_bounded_without_disabling_latest_evidence() {
    assert_eq!(
        (1..=6).map(retry_delay).collect::<Vec<_>>(),
        vec![30, 60, 120, 240, 300, 300]
    );
    assert_eq!(retry_delay(u64::MAX), 300);
}
