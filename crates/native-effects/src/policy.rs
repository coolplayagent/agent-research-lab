use anyhow::{Result, ensure};
use contracts::{NativeFilesystemAttempt, NativeFilesystemCategory, NativeFilesystemPath};
use std::path::{Component, Path, PathBuf};

/// Immutable host-selected roots and finite category limits; never worker input.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FilesystemRoot {
    pub id: u16,
    pub path: PathBuf,
    pub category: NativeFilesystemCategory,
    pub max_object_bytes: u64,
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FilesystemPolicy {
    pub roots: Vec<FilesystemRoot>,
    /// Exact host-confirmed attempts; labels never create admission authority.
    pub attempts: Vec<NativeFilesystemAttempt>,
    /// Opaque stores and evidence infrastructure cannot be entered by business IO.
    pub excluded: Vec<PathBuf>,
    /// At most four additional retained FDs per operation; existing caller FDs are separate.
    pub max_concurrent_operations: u32,
}
pub(crate) fn canonical(path: &Path) -> bool {
    path.is_absolute()
        && path
            .components()
            .all(|v| matches!(v, Component::RootDir | Component::Normal(_)))
        && !path
            .as_os_str()
            .as_encoded_bytes()
            .split(|b| *b == b'/')
            .any(|p| p == b"." || p == b"..")
}
impl FilesystemPolicy {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.roots.is_empty() && self.roots.len() <= 64,
            "filesystem root count"
        );
        ensure!(
            (1..=32).contains(&self.max_concurrent_operations),
            "filesystem operation/FD limit"
        );
        ensure!(self.excluded.len() <= 256, "filesystem exclusion count");
        ensure!(self.attempts.len() <= 32, "filesystem attempt count");
        let mut attempts = std::collections::BTreeSet::new();
        for a in &self.attempts {
            ensure!(
                (1..=3).contains(&a.attempt)
                    && [a.task_id.as_str(), a.run_id.as_str()]
                        .iter()
                        .all(|s| !s.is_empty()
                            && s.len() <= 128
                            && s.bytes()
                                .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b)))
                    && attempts.insert((&a.task_id, &a.run_id, a.attempt)),
                "filesystem attempt identity"
            );
        }
        let mut ids = std::collections::BTreeSet::new();
        let mut paths = std::collections::BTreeSet::new();
        for r in &self.roots {
            ensure!(
                canonical(&r.path)
                    && r.path != Path::new("/")
                    && ids.insert(r.id)
                    && paths.insert(&r.path),
                "filesystem root identity"
            );
            ensure!(
                r.max_object_bytes > 0
                    && r.max_object_bytes
                        <= match r.category {
                            NativeFilesystemCategory::Json
                            | NativeFilesystemCategory::Prompt
                            | NativeFilesystemCategory::Protocol
                            | NativeFilesystemCategory::Stream
                            | NativeFilesystemCategory::PrivateConfiguration => 16 * 1024 * 1024,
                            NativeFilesystemCategory::Cache
                            | NativeFilesystemCategory::Launcher => 512 * 1024 * 1024,
                            NativeFilesystemCategory::Directory => 1,
                        },
                "filesystem category bound"
            );
        }
        for p in &self.excluded {
            ensure!(canonical(p), "filesystem exclusion path");
        }
        Ok(())
    }
    pub(crate) fn classify(&self, path: &Path, bytes: u64) -> Result<NativeFilesystemPath> {
        ensure!(canonical(path), "noncanonical filesystem path");
        ensure!(
            !self.excluded.iter().any(|p| path.starts_with(p)),
            "excluded filesystem path"
        );
        let root = self
            .roots
            .iter()
            .filter(|r| path.starts_with(&r.path))
            .max_by_key(|r| r.path.components().count())
            .ok_or_else(|| anyhow::anyhow!("unapproved filesystem root"))?;
        ensure!(
            bytes <= root.max_object_bytes,
            "filesystem object reservation exceeds bound"
        );
        let relative = path
            .strip_prefix(&root.path)?
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("non-UTF8 filesystem locator"))?;
        ensure!(relative.len() <= 384, "filesystem locator exceeds bound");
        Ok(NativeFilesystemPath {
            root: root.id,
            relative: relative.into(),
            category: root.category,
        })
    }
}
