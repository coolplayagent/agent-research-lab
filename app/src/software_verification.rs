//! Shared acquisition budgets for candidate and merged-source verification.
use serde::{Deserialize, Deserializer, Serialize};
use std::path::Path;

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(transparent)]
pub(crate) struct SnapshotMaxFileMib(u8);

impl<'de> Deserialize<'de> for SnapshotMaxFileMib {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = u8::deserialize(deserializer)?;
        if !(1..=8).contains(&value) {
            return Err(serde::de::Error::custom(
                "qualitygate_snapshot_max_file_mib must be 1..=8",
            ));
        }
        Ok(Self(value))
    }
}

/// Omission retains the CLI default; an explicitly supplied value must be valid.
pub(crate) fn optional_budget<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<SnapshotMaxFileMib>, D::Error> {
    SnapshotMaxFileMib::deserialize(deserializer).map(Some)
}

pub(crate) fn qualitygate_command(
    program: String,
    source: &Path,
    diff: String,
    budget: Option<SnapshotMaxFileMib>,
) -> Vec<String> {
    let mut args = vec![
        program,
        "check".into(),
        "--root".into(),
        source.display().to_string(),
        "--diff".into(),
        diff,
        "--profile".into(),
        "full".into(),
        "--format".into(),
        "json".into(),
    ];
    if let Some(budget) = budget {
        args.extend(["--snapshot-max-file-mib".into(), budget.0.to_string()]);
    }
    args
}
