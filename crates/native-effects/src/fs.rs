//! Source-owned mutation boundary. Read-only std helpers remain unchanged.
use crate::{
    ObserverContext,
    reservation::{Operation, identity, io_error, perform, perform_options},
};
use contracts::NativeFilesystemOperation as Op;
pub use std::fs::{
    DirEntry, File, Metadata, Permissions, ReadDir, canonicalize, metadata, read, read_dir,
    read_link, read_to_string, symlink_metadata,
};
use std::{
    ffi::CString,
    io::{self, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::fs::{OpenOptionsExt, PermissionsExt},
    },
    path::{Component, Path, PathBuf},
};

pub fn guarded() -> bool {
    ObserverContext::current().filesystem.is_some()
}
pub(crate) fn reject(code: &str, error: io::Error) -> io::Error {
    if let Some(s) = ObserverContext::current().filesystem {
        s.fault(code)
    }
    error
}
fn c_name(p: &Path) -> io::Result<CString> {
    CString::new(p.as_os_str().as_encoded_bytes())
        .map_err(|_| io::Error::other("filesystem NUL locator"))
}
/// Retain one no-follow directory at a time, so an ancestor swap cannot redirect IO.
fn directory(path: &Path) -> io::Result<File> {
    if !crate::policy::canonical(path) {
        return Err(io::Error::other("noncanonical filesystem parent"));
    }
    let mut current = File::open("/")?;
    for part in path.components() {
        let Component::Normal(name) = part else {
            continue;
        };
        let name = CString::new(name.as_encoded_bytes())
            .map_err(|_| io::Error::other("filesystem NUL component"))?;
        let fd = unsafe {
            libc::openat(
                current.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        current = unsafe { File::from_raw_fd(fd) };
    }
    Ok(current)
}
fn parent(path: &Path) -> io::Result<(File, CString)> {
    let dir = directory(
        path.parent()
            .ok_or_else(|| io::Error::other("filesystem parent absent"))?,
    )?;
    let name = c_name(Path::new(
        path.file_name()
            .ok_or_else(|| io::Error::other("filesystem name absent"))?,
    ))?;
    Ok((dir, name))
}
fn returned(code: i32) -> io::Result<()> {
    if code < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct OpenOptions {
    inner: std::fs::OpenOptions,
    read: bool,
    write: bool,
    append: bool,
    truncate: bool,
    create: bool,
    create_new: bool,
    flags: i32,
    mode: u32,
}
impl Default for OpenOptions {
    fn default() -> Self {
        Self::new()
    }
}
impl OpenOptions {
    pub fn new() -> Self {
        Self {
            inner: std::fs::OpenOptions::new(),
            read: false,
            write: false,
            append: false,
            truncate: false,
            create: false,
            create_new: false,
            flags: 0,
            mode: 0o666,
        }
    }
    pub fn read(&mut self, v: bool) -> &mut Self {
        self.read = v;
        self.inner.read(v);
        self
    }
    pub fn write(&mut self, v: bool) -> &mut Self {
        self.write = v;
        self.inner.write(v);
        self
    }
    pub fn append(&mut self, v: bool) -> &mut Self {
        self.append = v;
        self.inner.append(v);
        self
    }
    pub fn truncate(&mut self, v: bool) -> &mut Self {
        self.truncate = v;
        self.inner.truncate(v);
        self
    }
    pub fn create(&mut self, v: bool) -> &mut Self {
        self.create = v;
        self.inner.create(v);
        self
    }
    pub fn create_new(&mut self, v: bool) -> &mut Self {
        self.create_new = v;
        self.inner.create_new(v);
        self
    }
    #[track_caller]
    pub fn open(&self, path: impl AsRef<Path>) -> io::Result<File> {
        let path = path.as_ref();
        if !guarded() {
            return self.inner.open(path);
        }
        let invalid = || io::Error::new(io::ErrorKind::InvalidInput, "invalid open options");
        if (!self.read && !self.write && !self.append)
            || ((!self.write && !self.append) && (self.truncate || self.create || self.create_new))
            || (self.append && self.truncate && !self.create_new)
        {
            return Err(reject("filesystem_open_options", invalid()));
        }
        // Source-owned callers use only these flags. Do not permit hidden O_CREAT/O_TMPFILE.
        let allowed = libc::O_ACCMODE | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC;
        if self.flags & !allowed != 0 {
            return Err(reject("filesystem_open_flags", invalid()));
        }
        if !(self.write || self.append || self.truncate || self.create || self.create_new) {
            return self.inner.open(path);
        }
        let access = match (self.read, self.write || self.append) {
            (true, true) => libc::O_RDWR,
            (false, true) => libc::O_WRONLY,
            (true, false) => libc::O_RDONLY,
            _ => unreachable!(),
        };
        let mut flags =
            access | (self.flags & !libc::O_ACCMODE) | libc::O_CLOEXEC | libc::O_NOFOLLOW;
        if self.append {
            flags |= libc::O_APPEND;
        }
        if self.truncate && !self.create_new {
            flags |= libc::O_TRUNC;
        }
        if self.create_new {
            return self.open_attempt(path, flags | libc::O_CREAT | libc::O_EXCL, Some(true));
        }
        if self.create {
            match self.open_attempt(path, flags | libc::O_CREAT | libc::O_EXCL, Some(true)) {
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
                result => return result,
            }
        }
        self.open_attempt(path, flags, Some(false))
    }
    #[track_caller]
    fn open_attempt(&self, path: &Path, flags: i32, created: Option<bool>) -> io::Result<File> {
        crate::reservation::open_observed(path, flags, self.mode, created, |mark| {
            let (dir, name) = parent(path)?;
            mark()?;
            let fd = unsafe { libc::openat(dir.as_raw_fd(), name.as_ptr(), flags, self.mode) };
            if fd < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(unsafe { File::from_raw_fd(fd) })
        })
    }
}
impl OpenOptionsExt for OpenOptions {
    fn mode(&mut self, mode: u32) -> &mut Self {
        self.mode = mode;
        self.inner.mode(mode);
        self
    }
    fn custom_flags(&mut self, flags: i32) -> &mut Self {
        self.flags = flags;
        self.inner.custom_flags(flags);
        self
    }
}

#[track_caller]
pub fn create_dir(path: impl AsRef<Path>) -> io::Result<()> {
    let p = path.as_ref();
    if !guarded() {
        return std::fs::create_dir(p);
    }
    perform_options(Op::CreateDirectory, &[p], 0, None, Some(0o777), |mark| {
        let (d, n) = parent(p)?;
        mark()?;
        returned(unsafe { libc::mkdirat(d.as_raw_fd(), n.as_ptr(), 0o777) })
    })
}
#[track_caller]
pub fn create_dir_all(path: impl AsRef<Path>) -> io::Result<()> {
    let p = path.as_ref();
    if !guarded() {
        return std::fs::create_dir_all(p);
    }
    if !crate::policy::canonical(p) {
        return Err(reject(
            "filesystem_directory_invalid",
            io::Error::other("noncanonical directory"),
        ));
    }
    let mut current = PathBuf::new();
    for component in p.components() {
        current.push(component.as_os_str());
        match std::fs::symlink_metadata(&current) {
            Ok(m) if m.is_dir() && !m.file_type().is_symlink() => {
                directory(&current).map_err(|e| reject("filesystem_directory_anchor", e))?;
            }
            Ok(_) => {
                return Err(reject(
                    "filesystem_directory_component",
                    io::Error::other("non-directory component"),
                ));
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => create_dir(&current)?,
            Err(e) => return Err(reject("filesystem_directory_metadata", e)),
        }
    }
    Ok(())
}
#[track_caller]
pub fn remove_file(path: impl AsRef<Path>) -> io::Result<()> {
    let p = path.as_ref();
    if !guarded() {
        return std::fs::remove_file(p);
    }
    perform_options(Op::Unlink, &[p], 0, Some(0), None, |mark| {
        let (d, n) = parent(p)?;
        mark()?;
        returned(unsafe { libc::unlinkat(d.as_raw_fd(), n.as_ptr(), 0) })
    })
}
#[track_caller]
pub fn remove_dir(path: impl AsRef<Path>) -> io::Result<()> {
    let p = path.as_ref();
    if !guarded() {
        return std::fs::remove_dir(p);
    }
    perform_options(
        Op::RemoveDirectory,
        &[p],
        0,
        Some(libc::AT_REMOVEDIR),
        None,
        |mark| {
            let (d, n) = parent(p)?;
            mark()?;
            returned(unsafe { libc::unlinkat(d.as_raw_fd(), n.as_ptr(), libc::AT_REMOVEDIR) })
        },
    )
}
#[track_caller]
pub fn remove_dir_all(path: impl AsRef<Path>) -> io::Result<()> {
    let p = path.as_ref();
    if !guarded() {
        return std::fs::remove_dir_all(p);
    }
    // Only the source-owned capture temporary's two known stream entries are eligible.
    let mut entries = std::fs::read_dir(p)
        .map_err(|e| reject("filesystem_cleanup_directory", e))?
        .take(4)
        .collect::<io::Result<Vec<_>>>()
        .map_err(|e| reject("filesystem_cleanup_listing", e))?;
    if entries.len() > 2
        || entries
            .iter()
            .any(|v| !matches!(v.file_name().to_str(), Some("stdout.jsonl" | "stderr.log")))
    {
        return Err(reject(
            "filesystem_cleanup_entries",
            io::Error::other("unexpected capture temporary entry"),
        ));
    }
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        remove_file(entry.path())?;
    }
    remove_dir(p)
}
#[track_caller]
pub fn rename(from: impl AsRef<Path>, to: impl AsRef<Path>) -> io::Result<()> {
    let (a, b) = (from.as_ref(), to.as_ref());
    if !guarded() {
        return std::fs::rename(a, b);
    }
    perform(Op::Rename, &[a, b], 0, |mark| {
        let (ad, an) = parent(a)?;
        let (bd, bn) = parent(b)?;
        mark()?;
        returned(unsafe {
            libc::renameat(ad.as_raw_fd(), an.as_ptr(), bd.as_raw_fd(), bn.as_ptr())
        })
    })
}
#[track_caller]
pub fn hard_link(from: impl AsRef<Path>, to: impl AsRef<Path>) -> io::Result<()> {
    let (a, b) = (from.as_ref(), to.as_ref());
    if !guarded() {
        return std::fs::hard_link(a, b);
    }
    perform_options(Op::HardLink, &[a, b], 0, Some(0), None, |mark| {
        let (ad, an) = parent(a)?;
        let (bd, bn) = parent(b)?;
        mark()?;
        returned(unsafe {
            libc::linkat(ad.as_raw_fd(), an.as_ptr(), bd.as_raw_fd(), bn.as_ptr(), 0)
        })
    })
}
pub(crate) fn same_file(file: &File, path: &Path) -> io::Result<()> {
    let (d, n) = parent(path)?;
    let fd = unsafe {
        libc::openat(
            d.as_raw_fd(),
            n.as_ptr(),
            libc::O_RDONLY | libc::O_NONBLOCK | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    let current = unsafe { File::from_raw_fd(fd) };
    let a = identity(&file.metadata()?);
    let b = identity(&current.metadata()?);
    if a.device != b.device
        || a.inode != b.inode
        || a.mode & libc::S_IFMT != b.mode & libc::S_IFMT
        || a.uid != b.uid
    {
        return Err(io::Error::other("filesystem retained inode differs"));
    }
    Ok(())
}
#[track_caller]
pub fn write_all(file: &mut File, path: &Path, bytes: &[u8]) -> io::Result<()> {
    if !guarded() {
        return file.write_all(bytes);
    }
    write_all_with(file, path, bytes, |file, bytes| file.write(bytes))
}
#[track_caller]
pub(crate) fn write_all_with(
    file: &mut File,
    path: &Path,
    bytes: &[u8],
    mut write: impl FnMut(&mut File, &[u8]) -> io::Result<usize>,
) -> io::Result<()> {
    let context = ObserverContext::current();
    let shared = context.filesystem.as_ref().expect("guarded context");
    shared
        .policy
        .classify(path, bytes.len() as u64)
        .map_err(|e| reject("filesystem_write_bound", io_error(e)))?;
    let current_size = file
        .metadata()
        .map_err(|e| reject("filesystem_write_metadata", e))?
        .len();
    shared
        .policy
        .classify(
            path,
            current_size
                .checked_add(bytes.len() as u64)
                .ok_or_else(|| {
                    reject(
                        "filesystem_write_size_overflow",
                        io::Error::other("filesystem size overflow"),
                    )
                })?,
        )
        .map_err(|e| reject("filesystem_write_growth_bound", io_error(e)))?;
    let mut remaining = bytes;
    while !remaining.is_empty() {
        let mut observation = Operation::begin(Op::Write, &[path], remaining.len() as u64)
            .map_err(io_error)?
            .expect("guarded operation");
        same_file(file, path)?;
        observation.before_effect().map_err(io_error)?;
        let result = write(file, remaining);
        let actual = Some(result.as_ref().copied().unwrap_or(0) as u64);
        observation.actual(&result, actual);
        let join = same_file(file, path);
        if join.is_err() {
            shared.fault("filesystem_write_inode_changed");
        }
        let finished = observation.finish().map_err(io_error);
        // Match std::Write::write_all, retaining each interrupted attempt as its own row.
        // The next reservation checks the original fence and budget again.
        let count = match result {
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {
                join?;
                finished?;
                continue;
            }
            // Observation failure is sticky, but must not replace the original syscall error.
            result => result?,
        };
        join?;
        finished?;
        if count == 0 {
            return Err(reject(
                "filesystem_write_zero",
                io::Error::new(io::ErrorKind::WriteZero, "filesystem write returned zero"),
            ));
        }
        remaining = &remaining[count..];
    }
    Ok(())
}
#[track_caller]
pub fn sync_all(file: &File, path: &Path) -> io::Result<()> {
    if !guarded() {
        return file.sync_all();
    }
    perform(Op::Sync, &[path], 0, |mark| {
        same_file(file, path)?;
        mark()?;
        file.sync_all()
    })
}
#[track_caller]
pub fn set_file_permissions(file: &File, path: &Path, permissions: Permissions) -> io::Result<()> {
    if !guarded() {
        return file.set_permissions(permissions);
    }
    perform_options(
        Op::Chmod,
        &[path],
        0,
        None,
        Some(permissions.mode()),
        |mark| {
            same_file(file, path)?;
            mark()?;
            file.set_permissions(permissions)
        },
    )
}
#[track_caller]
pub fn set_permissions(path: impl AsRef<Path>, permissions: Permissions) -> io::Result<()> {
    let p = path.as_ref();
    if !guarded() {
        return std::fs::set_permissions(p, permissions);
    }
    perform_options(Op::Chmod, &[p], 0, None, Some(permissions.mode()), |mark| {
        let (d, n) = parent(p)?;
        let fd = unsafe {
            libc::openat(
                d.as_raw_fd(),
                n.as_ptr(),
                libc::O_RDONLY | libc::O_NONBLOCK | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let f = unsafe { File::from_raw_fd(fd) };
        mark()?;
        returned(unsafe { libc::fchmod(f.as_raw_fd(), permissions.mode()) })
    })
}
/// Check the whole input bound before a truncating open, then preserve actual writes.
#[track_caller]
pub fn write(path: impl AsRef<Path>, bytes: impl AsRef<[u8]>) -> io::Result<()> {
    let p = path.as_ref();
    let bytes = bytes.as_ref();
    if !guarded() {
        return std::fs::write(p, bytes);
    }
    let context = ObserverContext::current();
    let shared = context.filesystem.as_ref().expect("guarded context");
    if let Err(e) = shared.policy.classify(p, bytes.len() as u64) {
        shared.fault("filesystem_write_bound");
        return Err(io_error(e));
    }
    let mut f = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(p)?;
    write_all(&mut f, p, bytes)
}

/// A finite serializer buffer; serialization is complete before create/truncate.
pub fn json_bytes<T: serde::Serialize>(
    path: &Path,
    value: &T,
    pretty: bool,
) -> anyhow::Result<Vec<u8>> {
    json_bytes_limit(path, value, pretty, usize::MAX)
}
pub fn json_bytes_limit<T: serde::Serialize>(
    path: &Path,
    value: &T,
    pretty: bool,
    requested_limit: usize,
) -> anyhow::Result<Vec<u8>> {
    let context = ObserverContext::current();
    let limit = if let Some(s) = &context.filesystem {
        if let Err(e) = s.policy.classify(path, 0) {
            s.fault("filesystem_serializer_path");
            return Err(e);
        }
        s.policy
            .roots
            .iter()
            .filter(|r| path.starts_with(&r.path))
            .max_by_key(|r| r.path.components().count())
            .expect("classified root")
            .max_object_bytes
            .min(16 * 1024 * 1024) as usize
    } else {
        usize::MAX
    }
    .min(requested_limit);
    struct Bounded {
        bytes: Vec<u8>,
        limit: usize,
    }
    impl Write for Bounded {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            if self
                .bytes
                .len()
                .checked_add(b.len())
                .is_none_or(|n| n > self.limit)
            {
                return Err(io::Error::other("filesystem JSON bound"));
            }
            self.bytes
                .try_reserve_exact(b.len())
                .map_err(io::Error::other)?;
            self.bytes.extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut out = Bounded {
        bytes: Vec::new(),
        limit,
    };
    let result = if pretty {
        serde_json::to_writer_pretty(&mut out, value)
    } else {
        serde_json::to_writer(&mut out, value)
    };
    if let Err(e) = result {
        if let Some(s) = context.filesystem {
            s.fault("filesystem_serialization_failed")
        }
        return Err(e.into());
    }
    Ok(out.bytes)
}

#[track_caller]
pub fn try_lock_exclusive(file: &File, path: &Path) -> io::Result<()> {
    perform_options(
        Op::Lock,
        &[path],
        0,
        Some(libc::LOCK_EX | libc::LOCK_NB),
        None,
        |mark| {
            if guarded() {
                same_file(file, path)?;
            }
            mark()?;
            returned(unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) })
        },
    )
}
#[track_caller]
pub fn unlock(file: &File, path: &Path) -> io::Result<()> {
    perform_options(Op::Unlock, &[path], 0, Some(libc::LOCK_UN), None, |mark| {
        if guarded() {
            same_file(file, path)?;
        }
        mark()?;
        returned(unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_UN) })
    })
}

#[track_caller]
pub fn check_write_bound(path: &Path, bytes: u64) -> io::Result<()> {
    if let Some(s) = ObserverContext::current().filesystem {
        s.policy
            .classify(path, bytes)
            .map_err(|e| reject("filesystem_write_bound", io_error(e)))?;
    }
    Ok(())
}
/// Preserve existing anchored communication traversal while observing its exact syscall.
#[track_caller]
pub fn mkdir_at(parent_fd: &File, path: &Path, name: &std::ffi::CStr, mode: u32) -> io::Result<()> {
    if guarded() && path.file_name().map(|n| n.as_encoded_bytes()) != Some(name.to_bytes()) {
        return Err(reject(
            "filesystem_at_name_mismatch",
            io::Error::other("filesystem name differs from approved path"),
        ));
    }
    perform_options(Op::CreateDirectory, &[path], 0, None, Some(mode), |mark| {
        if guarded() {
            same_file(
                parent_fd,
                path.parent()
                    .ok_or_else(|| io::Error::other("missing parent"))?,
            )?;
        }
        mark()?;
        returned(unsafe { libc::mkdirat(parent_fd.as_raw_fd(), name.as_ptr(), mode) })
    })
}
#[track_caller]
pub fn unlink_at(parent_fd: &File, path: &Path, name: &std::ffi::CStr) -> io::Result<()> {
    if guarded() && path.file_name().map(|n| n.as_encoded_bytes()) != Some(name.to_bytes()) {
        return Err(reject(
            "filesystem_at_name_mismatch",
            io::Error::other("filesystem name differs from approved path"),
        ));
    }
    perform_options(Op::Unlink, &[path], 0, Some(0), None, |mark| {
        if guarded() {
            same_file(
                parent_fd,
                path.parent()
                    .ok_or_else(|| io::Error::other("missing parent"))?,
            )?;
        }
        mark()?;
        returned(unsafe { libc::unlinkat(parent_fd.as_raw_fd(), name.as_ptr(), 0) })
    })
}

pub(crate) fn anchored_metadata(path: &Path) -> io::Result<contracts::NativeFilesystemIdentity> {
    let (dir, name) = parent(path)?;
    let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
    if unsafe {
        libc::fstatat(
            dir.as_raw_fd(),
            name.as_ptr(),
            stat.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    } < 0
    {
        return Err(io::Error::last_os_error());
    }
    let stat = unsafe { stat.assume_init() };
    Ok(contracts::NativeFilesystemIdentity {
        device: stat.st_dev,
        inode: stat.st_ino,
        mode: stat.st_mode,
        uid: stat.st_uid,
        links: stat.st_nlink,
        bytes: stat
            .st_size
            .try_into()
            .map_err(|_| io::Error::other("negative filesystem size"))?,
    })
}
