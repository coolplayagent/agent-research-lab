//! One locked owner for immutable requests and atomically replaced queue state.
use crate::protocol::{Operation, QueueEntry, QueueState};
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

pub(super) struct Outbox {
    root: PathBuf,
    _lock: storage::LockGuard,
}

impl Outbox {
    pub(super) fn open(root: &Path) -> Result<Self> {
        let lock = initialize(root)?;
        Ok(Self {
            root: fs::canonicalize(root)?,
            _lock: lock,
        })
    }

    pub(super) fn root(&self) -> &Path {
        &self.root
    }

    pub(super) fn save(&self, entry: &mut QueueEntry) -> Result<()> {
        self.validate_entry(entry)?;
        entry.updated_at = chrono::Utc::now().to_rfc3339();
        let path = self.root.join("states").join(format!("{}.json", entry.id));
        let temporary = path.with_extension("tmp");
        let mut file = File::create(&temporary)?;
        file.write_all(&serde_json::to_vec_pretty(entry)?)?;
        file.sync_all()?;
        fs::rename(temporary, path)?;
        File::open(self.root.join("states"))?.sync_all()?;
        Ok(())
    }

    pub(super) fn load(&self, id: &str) -> Result<QueueEntry> {
        delivery::validate_digest(id)?;
        let entry: QueueEntry = serde_json::from_slice(&fs::read(
            self.root.join("states").join(format!("{id}.json")),
        )?)?;
        ensure!(entry.id == id, "outbox state identity mismatch");
        self.validate_entry(&entry)?;
        Ok(entry)
    }

    pub(super) fn operation(&self, entry: &QueueEntry) -> Result<Operation> {
        self.validate_entry(entry)?;
        ensure!(
            fs::symlink_metadata(&entry.request_path)?
                .file_type()
                .is_file(),
            "outbox request must be a regular file"
        );
        let bytes = fs::read(&entry.request_path)?;
        ensure!(
            format!("{:x}", Sha256::digest(&bytes)) == entry.id,
            "immutable outbox request changed"
        );
        serde_json::from_slice(&bytes).context("invalid outbox operation")
    }

    pub(super) fn insert(&self, request: &Operation) -> Result<QueueEntry> {
        let bytes = serde_json::to_vec(request)?;
        let id = format!("{:x}", Sha256::digest(&bytes));
        if self.root.join("states").join(format!("{id}.json")).exists() {
            let existing = self.load(&id)?;
            self.operation(&existing)?;
            return Ok(existing);
        }
        let path = self.root.join("requests").join(format!("{id}.json"));
        if path.exists() {
            ensure!(fs::read(&path)? == bytes, "outbox request collision");
        } else {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&path, fs::Permissions::from_mode(0o400))?;
            }
            File::open(self.root.join("requests"))?.sync_all()?;
        }
        let mut entry = QueueEntry {
            id,
            state: QueueState::Pending,
            attempts: 0,
            request_path: path,
            result: None,
            error: None,
            updated_at: String::new(),
            retry_after_unix: 0,
        };
        self.save(&mut entry)?;
        Ok(entry)
    }

    pub(super) fn ids(&self) -> Result<Vec<String>> {
        let mut ids = Vec::new();
        for file in fs::read_dir(self.root.join("states"))? {
            let path = file?.path();
            if path.extension().is_some_and(|s| s == "json") {
                ids.push(
                    path.file_stem()
                        .context("missing state ID")?
                        .to_string_lossy()
                        .into_owned(),
                );
            }
        }
        ids.sort();
        Ok(ids)
    }

    fn validate_entry(&self, entry: &QueueEntry) -> Result<()> {
        delivery::validate_digest(&entry.id)?;
        ensure!(
            entry.request_path
                == self
                    .root
                    .join("requests")
                    .join(format!("{}.json", entry.id)),
            "outbox request path escaped its host directory"
        );
        Ok(())
    }
}

fn initialize(root: &Path) -> Result<storage::LockGuard> {
    for directory in [
        root.to_owned(),
        root.join("requests"),
        root.join("states"),
        root.join("evidence"),
    ] {
        if !directory.exists() {
            fs::create_dir_all(&directory)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
            }
        }
        ensure!(
            !fs::symlink_metadata(&directory)?.file_type().is_symlink(),
            "outbox directories cannot be symlinks"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            ensure!(
                fs::metadata(&directory)?.permissions().mode() & 0o077 == 0,
                "host outbox must not be accessible by other users"
            );
        }
    }
    storage::lock(&root.join("queue.lock"))
        .context("another host is processing the delivery outbox")
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
