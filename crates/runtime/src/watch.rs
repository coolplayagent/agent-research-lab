//! Host-owned liveness and bounded refresh recovery, independent of worker output.
use super::*;

pub(super) struct Controller {
    path: PathBuf,
    started: i64,
    process_start: String,
    last: Value,
}
impl Controller {
    pub(super) fn start(c: &Config) -> Result<Self> {
        let mut value = Self {
            path: c.state_dir.join("controller-status.json"),
            started: now(),
            process_start: process_start(std::process::id())?,
            last: Value::Null,
        };
        value.record("starting", &[])?;
        Ok(value)
    }
    pub(super) fn record(&mut self, phase: &str, active: &[String]) -> Result<()> {
        let time = now();
        if self.last["at"] == time
            && self.last["phase"] == phase
            && self.last["active"] == json!(active)
        {
            return Ok(());
        }
        self.last = json!({"schema_version":1,"pid":std::process::id(),"process_start":self.process_start,
            "started_at":self.started,"at":time,"phase":phase,"active":active});
        storage::write(&self.path, &self.last)
    }
}
impl Drop for Controller {
    fn drop(&mut self) {
        let _ = self.record("stopped", &[]);
    }
}

pub(super) fn next_seed(c: &Config) -> i64 {
    storage::read::<Value>(&c.state_dir.join("seed-status.json"))
        .ok()
        .and_then(|v| v["next_attempt_at"].as_i64())
        .unwrap_or(0)
}

/// A failed latest-input refresh remains fail-closed, but no longer kills startup
/// or leaves the service idle for fifteen minutes after an operator repairs tools.
pub(super) fn seed(c: &Config, operation: impl FnOnce() -> Result<Value>) -> Result<Value> {
    let path = c.state_dir.join("seed-status.json");
    seed_state(&path, operation)
}

fn seed_state(path: &Path, operation: impl FnOnce() -> Result<Value>) -> Result<Value> {
    let mut state = storage::read::<Value>(path).unwrap_or_else(|_| json!({"schema_version":1}));
    match operation() {
        Ok(result) => {
            let time = now();
            state["last_success_at"] = time.into();
            state["last_attempt_at"] = time.into();
            state["next_attempt_at"] = (time + 900).into();
            state["consecutive_failures"] = 0.into();
            state["error"] = Value::Null;
            state["queued"] = result["queued"].clone();
            storage::write(path, &state)?;
            Ok(result)
        }
        Err(error) => {
            let time = now();
            let failures = state["consecutive_failures"]
                .as_u64()
                .unwrap_or(0)
                .saturating_add(1);
            state["last_attempt_at"] = time.into();
            state["next_attempt_at"] = (time + retry_delay(failures)).into();
            state["consecutive_failures"] = failures.into();
            state["error"] = format!("{error:#}")
                .chars()
                .take(2000)
                .collect::<String>()
                .into();
            storage::write(path, &state)?;
            Err(error)
        }
    }
}

fn retry_delay(failures: u64) -> i64 {
    (30_i64 * (1 << failures.saturating_sub(1).min(4))).min(300)
}

#[cfg(test)]
mod tests {
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
                state["next_attempt_at"].as_i64().unwrap()
                    - state["last_attempt_at"].as_i64().unwrap(),
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
}
