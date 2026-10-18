//! Host-owned liveness and bounded refresh recovery, independent of worker output.
use crate::{host_budget::now, worker_identity::process_start};
use anyhow::Result;
use config::Config;
use contracts::ControllerPhase;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

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
        value.record(ControllerPhase::Starting, &[])?;
        Ok(value)
    }
    pub(super) fn record(&mut self, phase: ControllerPhase, active: &[String]) -> Result<()> {
        let time = now();
        if self.last["at"] == time
            && self.last["phase"] == phase.as_str()
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
        let _cleanup = native_effects::cleanup_scope();
        let _ = self.record(ControllerPhase::Stopped, &[]);
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
#[path = "watch_tests.rs"]
mod tests;
