//! One host authority store; shard locks remain held by the board operation.
use crate::{
    authority::{self, Cohort},
    files::{directory, private_directory, read_json, remove_view},
    limits::{
        CapacityKind, MAX_KNOWN_COHORTS, MAX_MEMBERS, MAX_RETAINED_COHORTS,
        MAX_RUN_OWNERS_PER_SHARD, MAX_STATE_BYTES, capacity,
    },
    projection::view_value,
    protocol::{Endpoint, HostMember, digest_id},
};
use anyhow::{Context, Result, ensure};
use native_effects::fs::{self, File};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Index {
    pub(super) schema_version: u32,
    pub(super) cohorts: BTreeMap<String, IndexEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct IndexEntry {
    pub(super) created_at: u64,
    pub(super) initialized: bool,
    pub(super) pruned: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RunIndex {
    pub(super) schema_version: u32,
    pub(super) bindings: BTreeMap<String, HostMember>,
}

pub(super) struct Store {
    state_dir: PathBuf,
    root: PathBuf,
}

impl Store {
    pub(super) fn state_dir(&self) -> &Path {
        &self.state_dir
    }

    pub(super) fn write_index(&self, shard: u8, index: &Index) -> Result<()> {
        storage::write(&self.index_path(shard), index)
    }

    /// Open existing host authority without creating directories or changing permissions.
    pub(super) fn open(state_dir: &Path) -> Result<Self> {
        ensure!(state_dir.is_absolute(), "board state path must be absolute");
        let root = state_dir.join("communication");
        let _ = directory(&root.join("authority"), false)?;
        Ok(Self {
            state_dir: state_dir.to_path_buf(),
            root,
        })
    }

    pub(super) fn new(state_dir: &Path) -> Result<Self> {
        ensure!(state_dir.is_absolute(), "board state path must be absolute");
        let _ = directory(state_dir, false)?;
        let root = state_dir.join("communication");
        for path in [
            &root,
            &root.join("authority"),
            &root.join("authority/cells"),
            &root.join("authority/runs"),
            &root.join("views"),
            &root.join("inboxes"),
        ] {
            private_directory(path)?;
        }
        Ok(Self {
            state_dir: state_dir.to_path_buf(),
            root,
        })
    }

    pub(super) fn index_path(&self, shard: u8) -> PathBuf {
        self.root
            .join("authority/cells")
            .join(format!("{shard:02x}"))
            .join("index.json")
    }

    pub(super) fn index(&self, shard: u8) -> Result<Index> {
        let index = read_json::<Index>(&self.index_path(shard))?.unwrap_or(Index {
            schema_version: 1,
            cohorts: BTreeMap::new(),
        });
        ensure!(
            index.schema_version == 1 && index.cohorts.len() <= MAX_KNOWN_COHORTS,
            "invalid board index"
        );
        for id in index.cohorts.keys() {
            ensure!(shard_of(id)? == shard, "cohort in wrong authority shard");
        }
        ensure!(
            index.cohorts.values().filter(|e| !e.pruned).count() <= MAX_RETAINED_COHORTS,
            "retained cohort limit exceeded"
        );
        Ok(index)
    }

    pub(super) fn lock(&self, id: &str) -> Result<storage::LockGuard> {
        let path = self.index_path(shard_of(id)?);
        private_directory(path.parent().unwrap())?;
        storage::lock(&path.with_file_name("shard.lock"))
    }

    pub(super) fn state_path(&self, id: &str) -> PathBuf {
        self.root
            .join("authority/cells")
            .join(&id[..2])
            .join(format!("{id}.json"))
    }

    pub(super) fn load(&self, id: &str) -> Result<Cohort> {
        digest_id(id)?;
        let index = self.index(shard_of(id)?)?;
        let entry = index.cohorts.get(id).context("cohort is not registered")?;
        ensure!(
            !entry.pruned,
            "cohort was pruned; its identity cannot be reused"
        );
        let c: Cohort = read_json(&self.state_path(id))?
            .context("cohort authority missing; replay host registration")?;
        authority::validate(&c, id)?;
        Ok(c)
    }

    pub(super) fn outbox(&self, run: &str) -> PathBuf {
        self.state_dir
            .join("runs")
            .join(run)
            .join("communication-outbox")
    }

    pub(super) fn endpoint(&self, member: &HostMember) -> Endpoint {
        Endpoint {
            outbox: self.outbox(&member.run_id),
            // Never nest private views under the legacy public mount: an older
            // running worker may still hold that entire directory read-only.
            view: self
                .root
                .join("inboxes")
                .join(&member.cohort_id)
                .join(&member.run_id),
        }
    }

    pub(super) fn save(&self, c: &Cohort) -> Result<()> {
        if fs::guarded() {
            fs::json_bytes_limit(&self.state_path(&c.id), c, true, MAX_STATE_BYTES)?;
            return storage::write(&self.state_path(&c.id), c);
        }
        ensure!(
            serde_json::to_vec_pretty(c)?.len() <= MAX_STATE_BYTES,
            "board state size limit reached"
        );
        storage::write(&self.state_path(&c.id), c)
    }

    pub(super) fn reserve_run(&self, member: &HostMember, existing_member: bool) -> Result<()> {
        let key = storage::digest(member.run_id.as_bytes());
        let root = self.root.join("authority/runs").join(&key[..2]);
        private_directory(&root)?;
        let _lock = storage::lock(&root.join("shard.lock"))?;
        let path = root.join("index.json");
        let mut index = read_json::<RunIndex>(&path)?.unwrap_or(RunIndex {
            schema_version: 1,
            bindings: BTreeMap::new(),
        });
        ensure!(
            index.schema_version == 1 && index.bindings.len() <= MAX_RUN_OWNERS_PER_SHARD,
            "invalid run authority index"
        );
        for (id, binding) in &index.bindings {
            binding.validate()?;
            ensure!(
                id == &storage::digest(binding.run_id.as_bytes()) && id[..2] == key[..2],
                "invalid run ownership key"
            );
        }
        if let Some(old) = index.bindings.get(&key) {
            ensure!(
                old == member,
                "run already belongs to another cohort or authority"
            );
            return Ok(());
        }
        ensure!(
            !existing_member,
            "committed run ownership record is missing"
        );
        capacity(
            index.bindings.len() < MAX_RUN_OWNERS_PER_SHARD,
            CapacityKind::RunOwners,
        )?;
        index.bindings.insert(key, member.clone());
        if fs::guarded() {
            fs::json_bytes_limit(&path, &index, true, MAX_STATE_BYTES)?;
        } else {
            capacity(
                serde_json::to_vec_pretty(&index)?.len() <= MAX_STATE_BYTES,
                CapacityKind::RunIndexBytes,
            )?;
        }
        storage::write(&path, &index)
    }

    pub(super) fn write_view(&self, c: &Cohort, now: u64) -> Result<()> {
        let view = self.root.join("views").join(&c.id);
        private_directory(&view)?;
        storage::write(&view.join("board.json"), &view_value(c, now, None)?)?;
        for member in &c.members {
            let view = self.endpoint(&member.identity).view;
            private_directory(&view)?;
            storage::write(
                &view.join("board.json"),
                &view_value(c, now, Some(&member.identity))?,
            )?;
        }
        Ok(())
    }

    pub(super) fn registration_state(&self, id: &str, index: &Index, now: u64) -> Result<Cohort> {
        Ok(match read_json::<Cohort>(&self.state_path(id))? {
            Some(_) => self.load(id)?,
            None => {
                ensure!(
                    !index.cohorts[id].initialized,
                    "committed cohort authority is missing"
                );
                Cohort {
                    schema_version: 1,
                    id: id.to_owned(),
                    revision: 0,
                    created_at: index.cohorts[id].created_at,
                    last_time: now,
                    closed_at: None,
                    cursor: 0,
                    members: vec![],
                    messages: vec![],
                }
            }
        })
    }

    pub(super) fn exports_intact(&self, c: &Cohort) -> bool {
        let paths = std::iter::once(self.root.join("views").join(&c.id))
            .chain(c.members.iter().map(|m| self.endpoint(&m.identity).view));
        paths.into_iter().all(|path| {
            read_json::<serde_json::Value>(&path.join("board.json"))
                .ok()
                .flatten()
                .is_some_and(|v| {
                    v["cohort_id"] == c.id && v["revision"].as_u64() == Some(c.revision)
                })
        })
    }

    pub(super) fn prune_files(&self, id: &str, shard: u8) -> Result<()> {
        // Tombstone commits first. Replaying after a crash finishes only our own exports.
        let inboxes = self.root.join("inboxes").join(id);
        match directory(&inboxes, false) {
            Ok(_) => {
                for (index, entry) in fs::read_dir(&inboxes)?.enumerate() {
                    ensure!(index < MAX_MEMBERS, "too many retained inboxes");
                    let entry = entry?;
                    ensure!(entry.file_type()?.is_dir(), "unexpected inbox entry");
                    remove_view(&entry.path())?;
                }
                fs::remove_dir(&inboxes)?;
            }
            Err(e)
                if e.downcast_ref::<std::io::Error>()
                    .is_some_and(|io| io.kind() == std::io::ErrorKind::NotFound) => {}
            Err(e) => return Err(e),
        }
        let view = self.root.join("views").join(id);
        remove_view(&view)?;
        match fs::remove_file(self.state_path(id)) {
            Ok(()) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.into()),
        }
        fs::sync_all(
            &File::open(self.index_path(shard).parent().unwrap())?,
            self.index_path(shard).parent().unwrap(),
        )?;
        Ok(())
    }
}

pub(super) fn shard_of(s: &str) -> Result<u8> {
    digest_id(s)?;
    Ok(u8::from_str_radix(&s[..2], 16)?)
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
