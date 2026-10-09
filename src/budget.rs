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
mod tests {
    use super::*;
    #[test]
    fn overlap_and_restart_are_counted_once() {
        let mut b = Budget::default();
        b.tick(100, true).unwrap();
        b.tick(130, true).unwrap();
        let mut b: Budget = serde_json::from_str(&serde_json::to_string(&b).unwrap()).unwrap();
        b.tick(160, false).unwrap();
        b.tick(200, false).unwrap();
        assert_eq!(b.days[&day(100)], 60);
    }
    #[test]
    fn crosses_singapore_midnight_without_reset() {
        let mut b = Budget::default();
        b.tick(57590, true).unwrap();
        b.tick(57610, false).unwrap();
        assert_eq!(b.days[&0], 10);
        assert_eq!(b.days[&1], 10);
        assert!(b.remaining(57610, 5) == 0);
    }
    #[test]
    fn clock_rollback_is_not_free_budget() {
        let mut b = Budget::default();
        b.tick(100, true).unwrap();
        assert!(b.tick(99, true).is_err());
    }
}
