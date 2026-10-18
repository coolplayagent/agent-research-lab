use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Persisted active wall time, split at Singapore midnight. Overlap is counted once.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Budget {
    pub days: BTreeMap<i64, u64>,
    pub active_since: Option<i64>,
}
pub fn day(timestamp: i64) -> i64 {
    (timestamp + 28800).div_euclid(86400)
}
impl Budget {
    pub fn tick(&mut self, now: i64, active: bool) -> Result<()> {
        if let Some(mut start) = self.active_since {
            if now < start {
                bail!("clock moved backwards; cannot reset budget");
            }
            while start < now {
                let d = day(start);
                let end = now.min((d + 1) * 86400 - 28800);
                *self.days.entry(d).or_default() += (end - start) as u64;
                start = end;
            }
        }
        self.active_since = active.then_some(now);
        Ok(())
    }
    pub fn remaining(&self, now: i64, limit: u64) -> u64 {
        limit.saturating_sub(*self.days.get(&day(now)).unwrap_or(&0))
    }
}
#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
