use crate::context::{CALLBACK, FilesystemReservation, ObserverContext, Shared};
use anyhow::{Result, ensure};
use contracts::*;
use std::{
    fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::Arc,
};

pub(crate) fn identity(m: &fs::Metadata) -> NativeFilesystemIdentity {
    NativeFilesystemIdentity {
        device: m.dev(),
        inode: m.ino(),
        mode: m.mode(),
        uid: m.uid(),
        links: m.nlink(),
        bytes: m.len(),
    }
}
fn metadata(path: &Path) -> std::io::Result<Option<NativeFilesystemIdentity>> {
    match crate::fs::anchored_metadata(path) {
        Ok(v) => Ok(Some(v)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

struct CallbackGuard;
impl CallbackGuard {
    fn enter() -> Result<Self> {
        ensure!(!CALLBACK.replace(true), "filesystem observer recursion");
        Ok(Self)
    }
}
impl Drop for CallbackGuard {
    fn drop(&mut self) {
        CALLBACK.set(false);
    }
}

pub(crate) struct Operation {
    shared: Arc<Shared>,
    reservation: Option<Box<dyn FilesystemReservation>>,
    actual: NativeFilesystemActual,
    paths: Vec<PathBuf>,
    class: NativeCaptureClass,
    finalized: bool,
    operation: NativeFilesystemOperation,
}
impl Operation {
    #[track_caller]
    pub fn begin(
        operation: NativeFilesystemOperation,
        paths: &[&Path],
        bytes: u64,
    ) -> Result<Option<Self>> {
        Self::begin_options(operation, paths, bytes, None, None)
    }
    #[track_caller]
    pub fn begin_options(
        operation: NativeFilesystemOperation,
        paths: &[&Path],
        bytes: u64,
        flags: Option<i32>,
        mode: Option<u32>,
    ) -> Result<Option<Self>> {
        let context = ObserverContext::current();
        let Some(shared) = context.filesystem.clone() else {
            return Ok(None);
        };
        let site = std::panic::Location::caller();
        let preparation = (|| {
            ensure!(!CALLBACK.get(), "filesystem observer recursion");
            shared.check(context.class())?;
            ensure!(
                paths.len() <= NATIVE_FILESYSTEM_PATHS_PER_OPERATION
                    && !paths.is_empty()
                    && bytes <= NATIVE_FILESYSTEM_REQUEST_BYTES,
                "filesystem path count"
            );
            let paths = paths
                .iter()
                .map(|p| shared.policy.classify(p, bytes))
                .collect::<Result<Vec<_>>>()?;
            let callsite = format!("{}:{}", site.file(), site.line());
            ensure!(callsite.len() <= 192, "filesystem callsite bound");
            let request = NativeFilesystemRequest {
                operation,
                class: context.class(),
                scope: context.scope,
                attempt: context.attempt.clone(),
                callsite,
                paths,
                requested_bytes: bytes,
                control_span: context.control_span,
                flags,
                mode,
            };
            let started = shared.fence.now_ns()?;
            {
                let mut state = shared
                    .state
                    .lock()
                    .map_err(|_| anyhow::anyhow!("filesystem state poisoned"))?;
                ensure!(
                    state.outstanding < shared.policy.max_concurrent_operations,
                    "filesystem concurrent FD budget"
                );
                ensure!(
                    state.reserved < NATIVE_FILESYSTEM_EVENTS,
                    "filesystem event bound"
                );
                match context.class() {
                    NativeCaptureClass::Normal => {
                        ensure!(
                            state.normal_reserved < NATIVE_FILESYSTEM_NORMAL_EVENTS,
                            "filesystem normal event bound"
                        );
                        state.normal_reserved += 1;
                    }
                    NativeCaptureClass::Cleanup => {
                        ensure!(
                            state.cleanup_reserved < NATIVE_FILESYSTEM_CLEANUP_EVENTS,
                            "filesystem cleanup event bound"
                        );
                        state.cleanup_reserved += 1;
                    }
                }
                state.reserved = state
                    .reserved
                    .checked_add(1)
                    .ok_or_else(|| anyhow::anyhow!("filesystem ordinal overflow"))?;
                state.outstanding += 1;
            }
            Ok((request, started))
        })();
        let (request, started) = match preparation {
            Ok(v) => v,
            Err(e) => {
                shared.fault("filesystem_precondition_failed");
                return Err(e);
            }
        };
        // From this point Drop must retire the reservation even if metadata or the sink fails.
        let mut value = Self {
            shared,
            reservation: None,
            actual: NativeFilesystemActual {
                result: NativeFilesystemResult::NotAttempted,
                errno: None,
                written_bytes: None,
                return_value: None,
                created: None,
                before: Vec::new(),
                after: Vec::new(),
                started_ns: started,
                ended_ns: None,
            },
            paths: paths.iter().map(|p| (*p).to_owned()).collect(),
            class: context.class(),
            finalized: false,
            operation,
        };
        let token = {
            let _callback = CallbackGuard::enter()?;
            value.shared.observer.reserve(&request)?
        };
        value.reservation = Some(token);
        value.actual.before = value
            .paths
            .iter()
            .map(|p| metadata(p))
            .collect::<std::io::Result<_>>()?;
        {
            let state = value
                .shared
                .state
                .lock()
                .map_err(|_| anyhow::anyhow!("filesystem state poisoned"))?;
            for (path, seen) in value.paths.iter().zip(&value.actual.before) {
                if let Some(seen) = seen {
                    let kind = seen.mode & libc::S_IFMT;
                    ensure!(
                        kind == libc::S_IFREG || kind == libc::S_IFDIR,
                        "filesystem special entry rejected"
                    );
                    if kind == libc::S_IFREG {
                        value.shared.policy.classify(path, seen.bytes)?;
                    }
                }
                if let Some(known) = state.known.get(path) {
                    ensure!(
                        match (known, seen) {
                            (Some(known), Some(seen)) =>
                                seen.device == known.device && seen.inode == known.inode,
                            (None, None) => true,
                            _ => false,
                        },
                        "filesystem previously observed inode changed"
                    );
                }
            }
        }
        value.shared.check(value.class)?;
        Ok(Some(value))
    }
    pub fn before_effect(&mut self) -> Result<()> {
        self.shared.check(self.class)?;
        self.actual.result = NativeFilesystemResult::Unknown;
        Ok(())
    }
    pub fn actual<T>(&mut self, result: &std::io::Result<T>, bytes: Option<u64>) {
        self.actual.result = if result.is_ok() {
            NativeFilesystemResult::Succeeded
        } else {
            NativeFilesystemResult::Failed
        };
        self.actual.errno = result.as_ref().err().and_then(std::io::Error::raw_os_error);
        self.actual.written_bytes = bytes;
        if let Err(e) = result
            && !self.operation.permits_failure(e.raw_os_error())
        {
            self.shared.fault("filesystem_os_failure");
        }
        self.actual.return_value = match result {
            Ok(_) => Some(bytes.map_or(0, |v| v as i64)),
            Err(e) => e.raw_os_error().map(|_| -1),
        };
    }
    pub fn returned(&mut self, value: Option<i64>, created: Option<bool>) {
        self.actual.return_value = value;
        self.actual.created = created;
    }
    pub fn finish(mut self) -> Result<()> {
        let completion = (|| {
            self.actual.after = self
                .paths
                .iter()
                .map(|p| metadata(p))
                .collect::<std::io::Result<_>>()?;
            if self.actual.result == NativeFilesystemResult::Succeeded {
                self.validate_transition()?;
                let mut state = self
                    .shared
                    .state
                    .lock()
                    .map_err(|_| anyhow::anyhow!("filesystem state poisoned"))?;
                for (path, after) in self.paths.iter().zip(&self.actual.after) {
                    ensure!(
                        state.known.contains_key(path)
                            || state.known.len() < NATIVE_FILESYSTEM_PATHS,
                        "filesystem identity table bound"
                    );
                    // Retain absence, too: a later same-name occupant is not owned cleanup.
                    state.known.insert(path.clone(), after.clone());
                }
            }
            self.actual.ended_ns = Some(self.shared.fence.now_ns()?);
            ensure!(
                self.actual.ended_ns >= Some(self.actual.started_ns),
                "filesystem clock order"
            );
            self.shared.check(self.class)?;
            Ok(())
        })();
        if completion.is_err() {
            self.shared.fault("filesystem_poststate_failed");
        }
        let publication = self.publish();
        completion.and(publication)
    }
    fn validate_transition(&self) -> Result<()> {
        use NativeFilesystemOperation::*;
        let same = |a: &Option<NativeFilesystemIdentity>, b: &Option<NativeFilesystemIdentity>| {
            a.as_ref()
                .zip(b.as_ref())
                .is_some_and(|(a, b)| a.device == b.device && a.inode == b.inode)
        };
        match self.operation {
            Rename => ensure!(
                self.actual.after[0].is_none()
                    && same(&self.actual.before[0], &self.actual.after[1]),
                "filesystem rename identity"
            ),
            HardLink => ensure!(
                same(&self.actual.before[0], &self.actual.after[0])
                    && same(&self.actual.before[0], &self.actual.after[1]),
                "filesystem hardlink identity"
            ),
            Unlink | RemoveDirectory => ensure!(
                self.actual.after[0].is_none(),
                "filesystem removed entry returned"
            ),
            OpenWrite if self.actual.created == Some(true) => ensure!(
                self.actual.after[0].is_some(),
                "filesystem created entry absent"
            ),
            OpenWrite if self.actual.before[0].is_none() => ensure!(
                self.actual.after[0].is_some(),
                "filesystem opened entry absent"
            ),
            CreateDirectory => ensure!(
                self.actual.after[0].is_some(),
                "filesystem directory absent"
            ),
            _ => ensure!(
                same(&self.actual.before[0], &self.actual.after[0]),
                "filesystem operation inode changed"
            ),
        }
        Ok(())
    }
    fn publish(&mut self) -> Result<()> {
        ensure!(!self.finalized, "filesystem operation finalized twice");
        self.finalized = true;
        let publication = match self.reservation.take() {
            Some(r) => {
                let _callback = CallbackGuard::enter()?;
                r.finish(&self.actual)
            }
            None => Err(anyhow::anyhow!("filesystem reservation absent")),
        };
        let mut s = self
            .shared
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("filesystem state poisoned"))?;
        s.outstanding = s
            .outstanding
            .checked_sub(1)
            .ok_or_else(|| anyhow::anyhow!("filesystem operation underflow"))?;
        s.finalized = s
            .finalized
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("filesystem completion overflow"))?;
        if publication.is_err() {
            s.fault
                .get_or_insert("filesystem_publication_failed".into());
        }
        publication
    }
}
impl Drop for Operation {
    fn drop(&mut self) {
        if !self.finalized {
            self.shared.fault("filesystem_operation_abandoned");
            let _ = self.publish();
        }
    }
}
pub(crate) fn io_error(e: anyhow::Error) -> std::io::Error {
    std::io::Error::other(e.to_string())
}

#[track_caller]
pub(crate) fn perform<T>(
    operation: NativeFilesystemOperation,
    paths: &[&Path],
    bytes: u64,
    action: impl FnOnce(&mut dyn FnMut() -> std::io::Result<()>) -> std::io::Result<T>,
) -> std::io::Result<T> {
    perform_options(operation, paths, bytes, None, None, action)
}
#[track_caller]
pub(crate) fn perform_options<T>(
    operation: NativeFilesystemOperation,
    paths: &[&Path],
    bytes: u64,
    flags: Option<i32>,
    mode: Option<u32>,
    action: impl FnOnce(&mut dyn FnMut() -> std::io::Result<()>) -> std::io::Result<T>,
) -> std::io::Result<T> {
    let mut observation =
        Operation::begin_options(operation, paths, bytes, flags, mode).map_err(io_error)?;
    let mut attempted = false;
    let actual = action(&mut || {
        if let Some(o) = &mut observation {
            o.before_effect().map_err(io_error)?;
        }
        attempted = true;
        Ok(())
    });
    if let Some(mut o) = observation {
        if attempted {
            o.actual(&actual, None);
        } else {
            o.shared.fault("filesystem_effect_not_attempted");
        }
        let publication = o.finish().map_err(io_error);
        if actual.is_ok() {
            publication?;
        }
    }
    actual
}
#[track_caller]
pub(crate) fn open_observed(
    path: &Path,
    flags: i32,
    mode: u32,
    created: Option<bool>,
    action: impl FnOnce(&mut dyn FnMut() -> std::io::Result<()>) -> std::io::Result<std::fs::File>,
) -> std::io::Result<std::fs::File> {
    use std::os::fd::AsRawFd;
    let mut observation = Operation::begin_options(
        NativeFilesystemOperation::OpenWrite,
        &[path],
        0,
        Some(flags),
        Some(mode),
    )
    .map_err(io_error)?;
    let mut attempted = false;
    let result = action(&mut || {
        if let Some(o) = &mut observation {
            o.before_effect().map_err(io_error)?;
        }
        attempted = true;
        Ok(())
    });
    if let Some(mut o) = observation {
        if attempted {
            o.actual(&result, None);
            o.returned(
                match &result {
                    Ok(f) => Some(i64::from(f.as_raw_fd())),
                    Err(e) => e.raw_os_error().map(|_| -1),
                },
                result.as_ref().ok().and(created),
            );
        } else {
            o.shared.fault("filesystem_open_not_attempted");
        }
        let inode_join = result
            .as_ref()
            .ok()
            .map(|f| crate::fs::same_file(f, path))
            .transpose();
        if inode_join.is_err() {
            o.shared.fault("filesystem_open_inode_changed");
        }
        let publication = o.finish().map_err(io_error);
        if result.is_ok() {
            inode_join?;
            publication?;
        }
    }
    result
}
