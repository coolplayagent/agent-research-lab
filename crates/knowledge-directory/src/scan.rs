//! Descriptor-relative, bounded content snapshots with explicit repository ignore rules.
use super::{anchored, child, read};
use anyhow::{Context, Result, ensure};
use contracts::{KnowledgeError as Error, KnowledgeErrorCode as Code};
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
const VIRTUAL_ROOT: &str = "/knowledge";
const MAX_ENTRIES: usize = 65_536;
const MAX_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const MAX_DEPTH: usize = 32;

pub(super) struct Budget {
    deadline: Instant,
    entries: usize,
    bytes: u64,
    max_entries: usize,
    max_bytes: u64,
}
impl Budget {
    pub fn new() -> Self {
        Self {
            deadline: Instant::now() + Duration::from_secs(45),
            entries: 0,
            bytes: 0,
            max_entries: MAX_ENTRIES,
            max_bytes: MAX_BYTES,
        }
    }
    fn check(&self, path: &Path) -> Result<()> {
        ensure!(
            Instant::now() < self.deadline,
            Error::new(Code::Timeout)
                .parameter("path", path.display())
                .parameter("limit", 45)
        );
        Ok(())
    }
    fn entry(&mut self, path: &Path) -> Result<()> {
        self.check(path)?;
        self.entries += 1;
        ensure!(
            self.entries <= self.max_entries,
            Error::new(Code::FileLimit)
                .parameter("path", path.display())
                .parameter("actual", self.entries)
                .parameter("limit", self.max_entries)
        );
        Ok(())
    }
    pub(super) fn consume(&mut self, path: &Path, bytes: usize) -> Result<()> {
        self.check(path)?;
        self.bytes = self.bytes.saturating_add(bytes as u64);
        ensure!(
            self.bytes <= self.max_bytes,
            Error::new(Code::SnapshotLimit)
                .parameter("path", path.display())
                .parameter("actual", self.bytes)
                .parameter("limit", self.max_bytes)
        );
        Ok(())
    }
}
fn reserved(path: &Path) -> bool {
    path.components()
        .any(|c| c.as_os_str() == ".git" || c.as_os_str() == ".outcomehub")
}
fn ignored(filters: &[Gitignore], path: &Path, directory: bool) -> bool {
    let absolute = Path::new(VIRTUAL_ROOT).join(path);
    let mut excluded = false;
    for filter in filters {
        let matched = filter.matched(&absolute, directory);
        if matched.is_ignore() {
            excluded = true;
        }
        if matched.is_whitelist() {
            excluded = false;
        }
    }
    excluded
}
fn rules(root: &File, prefix: &Path, budget: &mut Budget) -> Result<(Gitignore, Option<String>)> {
    budget.check(prefix)?;
    let content = read(root, Path::new(".gitignore"))?;
    let mut builder = GitignoreBuilder::new(Path::new(VIRTUAL_ROOT).join(prefix));
    let digest = if let Some(bytes) = content {
        budget.consume(&prefix.join(".gitignore"), bytes.len())?;
        let text = std::str::from_utf8(&bytes).map_err(|_| {
            Error::new(Code::IgnoreInvalid).parameter("path", prefix.join(".gitignore").display())
        })?;
        for (index, line) in text.lines().enumerate() {
            budget.check(prefix)?;
            ensure!(
                index < 4096 && line.len() <= 4096,
                Error::new(Code::IgnoreInvalid)
                    .parameter("path", prefix.join(".gitignore").display())
            );
            builder.add_line(None, line).map_err(|_| {
                Error::new(Code::IgnoreInvalid)
                    .parameter("path", prefix.join(".gitignore").display())
            })?;
        }
        Some(storage::digest(&bytes))
    } else {
        None
    };
    Ok((
        builder
            .build()
            .map_err(|_| Error::new(Code::IgnoreInvalid).parameter("path", prefix.display()))?,
        digest,
    ))
}
/// Validate the same policy for direct reads and future publication paths, including missing parents.
pub(super) fn allowed(root: &File, path: &Path, budget: &mut Budget) -> Result<()> {
    ensure!(
        !reserved(path),
        Error::new(Code::ExcludedPath).parameter("path", path.display())
    );
    knowledge_model::relative(path)?;
    let mut filters = Vec::new();
    let mut current = Some(root.try_clone()?);
    let mut prefix = PathBuf::new();
    let components = path.components().collect::<Vec<_>>();
    for (index, component) in components.iter().enumerate() {
        if let Some(directory) = &current {
            filters.push(rules(directory, &prefix, budget)?.0);
        }
        prefix.push(component.as_os_str());
        let is_dir = index + 1 < components.len();
        ensure!(
            !ignored(&filters, &prefix, is_dir),
            Error::new(Code::ExcludedPath).parameter("path", path.display())
        );
        if is_dir {
            current = match current {
                Some(directory) => match child(&directory, Path::new(component.as_os_str()), false)
                {
                    Ok(next) => Some(next),
                    Err(error)
                        if error
                            .downcast_ref::<std::io::Error>()
                            .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) =>
                    {
                        None
                    }
                    Err(error) => return Err(error),
                },
                None => None,
            };
        }
    }
    Ok(())
}
fn identity(meta: &fs::Metadata) -> (u64, u64, u64, i64, i64, i64, i64) {
    (
        meta.dev(),
        meta.ino(),
        meta.len(),
        meta.mtime(),
        meta.mtime_nsec(),
        meta.ctime(),
        meta.ctime_nsec(),
    )
}
fn hash(root: &File, name: &Path, path: &Path, budget: &mut Budget) -> Result<String> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(anchored(root).join(name))?;
    let before = file.metadata()?;
    ensure!(
        before.is_file(),
        Error::new(Code::SpecialFileRejected).parameter("path", path.display())
    );
    let mut digest = Sha256::new();
    let mut bytes = [0u8; 65_536];
    loop {
        budget.check(path)?;
        let count = file.read(&mut bytes)?;
        if count == 0 {
            break;
        }
        budget.consume(path, count)?;
        digest.update(&bytes[..count]);
    }
    let after = file.metadata()?;
    let linked = fs::symlink_metadata(anchored(root).join(name))?;
    ensure!(
        identity(&before) == identity(&after) && identity(&after) == identity(&linked),
        Error::new(Code::ContentChanged).parameter("path", path.display())
    );
    Ok(format!("{:x}", digest.finalize()))
}
pub(super) fn snapshot(root: &File, budget: &mut Budget) -> Result<String> {
    fn walk(
        root: &File,
        prefix: &Path,
        filters: &mut Vec<Gitignore>,
        entries: &mut BTreeMap<String, String>,
        budget: &mut Budget,
        depth: usize,
    ) -> Result<()> {
        budget.check(prefix)?;
        ensure!(
            depth <= MAX_DEPTH,
            Error::new(Code::DepthLimit)
                .parameter("path", prefix.display())
                .parameter("actual", depth)
                .parameter("limit", MAX_DEPTH)
        );
        let (filter, digest) = rules(root, prefix, budget)?;
        filters.push(filter);
        if let Some(digest) = digest {
            entries.insert(
                prefix
                    .join(".gitignore")
                    .to_str()
                    .context("knowledge_invalid_path")?
                    .into(),
                digest,
            );
        }
        // Do not collect an unbounded directory listing before applying the entry budget.
        for entry in fs::read_dir(anchored(root))? {
            let entry = entry?;
            let name = PathBuf::from(entry.file_name());
            let path = prefix.join(&name);
            budget.entry(&path)?;
            if reserved(&path) || name == Path::new(".gitignore") {
                continue;
            }
            knowledge_model::relative(&name)?;
            let meta = fs::symlink_metadata(entry.path())?;
            if ignored(filters, &path, meta.is_dir()) {
                continue;
            }
            ensure!(
                !meta.file_type().is_symlink(),
                Error::new(Code::SymlinkRejected).parameter("path", path.display())
            );
            if meta.is_dir() {
                entries.insert(
                    format!("{}/", path.to_str().context("knowledge_invalid_path")?),
                    String::new(),
                );
                walk(
                    &child(root, &name, false)?,
                    &path,
                    filters,
                    entries,
                    budget,
                    depth + 1,
                )?;
            } else {
                ensure!(
                    meta.is_file(),
                    Error::new(Code::SpecialFileRejected).parameter("path", path.display())
                );
                let digest = hash(root, &name, &path, budget)?;
                entries.insert(
                    path.to_str().context("knowledge_invalid_path")?.into(),
                    digest,
                );
            }
        }
        filters.pop();
        Ok(())
    }
    let mut entries = BTreeMap::new();
    walk(
        root,
        Path::new(""),
        &mut Vec::new(),
        &mut entries,
        budget,
        0,
    )?;
    Ok(format!(
        "directory-v2:{}",
        storage::digest(&serde_json::to_vec(&entries)?)
    ))
}

#[cfg(test)]
#[path = "scan_tests.rs"]
mod tests;
