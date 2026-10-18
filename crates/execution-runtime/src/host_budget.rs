//! Persisted daily allowances and bounded host-operation windows.
use anyhow::Result;
use budget::Budget;
use config::Config;
use native_effects::fs;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::Duration;

#[derive(Default, Serialize, Deserialize)]
pub(super) struct Ledger {
    pub(super) budget: Budget,
    pub(super) main_seconds: u64,
    pub(super) exploration_seconds: u64,
    pub(super) exploration_percent: u8,
}

pub(super) fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

pub fn pause(c: &Config, paused: bool) -> Result<Value> {
    fs::create_dir_all(&c.state_dir)?;
    let path = c.state_dir.join("paused");
    if paused {
        storage::write(&path, &json!({"requested_at":now()}))?;
    } else if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(json!({"paused":paused}))
}

#[derive(Clone, Copy)]
pub(super) struct RunWindow {
    pub(super) started: i64,
    pub(super) max_seconds: u64,
}

impl RunWindow {
    pub(super) fn remaining(self, c: &Config, ledger: &Ledger) -> u64 {
        let current = now();
        if c.state_dir.join("paused").exists() {
            return 0;
        }
        ledger
            .budget
            .remaining(current, c.daily_seconds)
            .min(if self.max_seconds == 0 {
                u64::MAX
            } else {
                self.max_seconds
                    .saturating_sub((current - self.started).max(0) as u64)
            })
    }
}

pub(super) fn load_ledger(c: &Config) -> Result<Ledger> {
    let path = c.state_dir.join("budget.json");
    let mut ledger = if path.exists() {
        storage::read(&path)?
    } else {
        Ledger {
            exploration_percent: 25,
            ..Default::default()
        }
    };
    // An interrupted active interval is conservatively charged through restart.
    ledger.budget.tick(now(), false)?;
    storage::write(&path, &ledger)?;
    Ok(ledger)
}

/// Host work shares the daily wall-clock allowance with model execution.
/// Persist before effects, and charge even when the adapter returns an error.
pub(super) fn idle_host<T>(
    c: &Config,
    ledger: &mut Ledger,
    window: RunWindow,
    operation: impl FnOnce() -> Result<T>,
) -> Result<Option<T>> {
    let remaining = window.remaining(c, ledger);
    if remaining == 0 {
        return Ok(None);
    }
    let started = now();
    ledger.budget.tick(started, true)?;
    storage::write(&c.state_dir.join("budget.json"), ledger)?;
    let result = {
        let _deadline = process::deadline_scope(Duration::from_secs(remaining));
        operation()
    };
    let completed = now();
    ledger.main_seconds = ledger
        .main_seconds
        .saturating_add((completed - started).max(0) as u64);
    ledger.budget.tick(completed, false)?;
    storage::write(&c.state_dir.join("budget.json"), ledger)?;
    result.map(Some)
}

pub(super) fn prepare_host<T>(
    c: &Config,
    ledger: &mut Ledger,
    window: RunWindow,
    active: bool,
    operation: impl FnOnce() -> Result<T>,
) -> Result<Option<T>> {
    if !active {
        return idle_host(c, ledger, window, operation);
    }
    ledger.budget.tick(now(), true)?;
    storage::write(&c.state_dir.join("budget.json"), ledger)?;
    let remaining = window.remaining(c, ledger);
    if remaining == 0 {
        return Ok(None);
    }
    let _deadline = process::deadline_scope(Duration::from_secs(remaining.min(10)));
    operation().map(Some)
}

/// Unclaimed prerequisites are safe to defer. A host budget ending during a
/// probe is not evidence that the installed tool or task is permanently broken.
pub(super) fn prepare_prerequisite<T>(
    c: &Config,
    ledger: &mut Ledger,
    window: RunWindow,
    active: bool,
    operation: impl FnOnce() -> Result<T>,
) -> Result<Option<T>> {
    prepare_host(c, ledger, window, active, || match operation() {
        Ok(value) => Ok(Some(value)),
        Err(_) if process::deadline_exhausted() => Ok(None),
        Err(error) => Err(error),
    })
    .map(Option::flatten)
}

#[cfg(test)]
#[path = "host_budget_tests.rs"]
mod tests;
