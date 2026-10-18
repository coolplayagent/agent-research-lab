use super::*;
use crate::tests::completion_fixture;
use config::safe_id;
use serde_json::json;

#[test]
fn frozen_attempt_limits_preserve_legacy_serialization_and_stop_retries() {
    let (temp, c, mut job, _) = completion_fixture(false, json!([]));
    job.task.write = false;
    let legacy = serde_json::to_value(&job.task).unwrap();
    assert!(legacy.get("max_attempts").is_none());
    assert_eq!(attempt_limit(&job.task), 3);
    for attempt in 1..=3 {
        job.attempt = attempt;
        defer_read_retry(&mut job, "observed failure".into());
        assert_eq!(job.retry_after.is_some(), attempt < 3);
    }
    job.attempt = 1;
    job.task.max_attempts = Some(1);
    defer_read_retry(&mut job, "observed failure".into());
    assert!(job.retry_after.is_none());
    assert!(prepare_retry(&c, &mut job).is_err());
    assert_eq!(job.attempt, 1);
    for limit in [0, 4] {
        job.task.id = format!("invalid-{limit}");
        job.task.max_attempts = Some(limit);
        assert!(enqueue_with_inputs(&c, job.task.clone(), None).is_err());
    }
    drop(temp);
}

#[test]
fn identifiers_cannot_escape_state() {
    assert!(safe_id("../../secrets").is_err());
    assert!(safe_id("research-123").is_ok());
}
