//! Windows persistence. Reparse points are rejected for every existing path component.
use anyhow::{Context, Result, bail, ensure};
use fs2::FileExt;
use serde::{Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::ffi::c_void;
use std::{
    ffi::OsStr,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::windows::{
        ffi::OsStrExt,
        fs::{MetadataExt, OpenOptionsExt},
    },
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

const REPARSE_POINT: u32 = 0x400;
const OPEN_REPARSE_POINT: u32 = 0x0020_0000;
const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
const MOVEFILE_WRITE_THROUGH: u32 = 0x8;
static NEXT: AtomicU64 = AtomicU64::new(0);

#[link(name = "Kernel32")]
unsafe extern "system" {
    fn MoveFileExW(existing: *const u16, replacement: *const u16, flags: u32) -> i32;
    fn LocalFree(memory: *mut c_void) -> *mut c_void;
}

#[link(name = "Advapi32")]
unsafe extern "system" {
    fn ConvertStringSecurityDescriptorToSecurityDescriptorW(
        sddl: *const u16,
        revision: u32,
        descriptor: *mut *mut c_void,
        size: *mut u32,
    ) -> i32;
    fn SetFileSecurityW(path: *const u16, information: u32, descriptor: *mut c_void) -> i32;
}

fn wide(path: &OsStr) -> Vec<u16> {
    path.encode_wide().chain(std::iter::once(0)).collect()
}

fn reject_reparse(path: &Path) -> Result<()> {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        let metadata = match fs::symlink_metadata(&current) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        ensure!(
            metadata.file_attributes() & REPARSE_POINT == 0,
            "reparse point in state path"
        );
    }
    Ok(())
}

fn restrict_to_owner(path: &Path, directory: bool) -> Result<()> {
    reject_reparse(path)?;
    let sddl = if directory {
        "D:P(A;OICI;FA;;;OW)(A;OICI;FA;;;SY)"
    } else {
        "D:P(A;;FA;;;OW)(A;;FA;;;SY)"
    };
    let mut descriptor = std::ptr::null_mut();
    let converted = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            wide(OsStr::new(sddl)).as_ptr(),
            1,
            &mut descriptor,
            std::ptr::null_mut(),
        )
    };
    if converted == 0 {
        return Err(std::io::Error::last_os_error())
            .context("build private state access descriptor");
    }
    let result = unsafe { SetFileSecurityW(wide(path.as_os_str()).as_ptr(), 0x4, descriptor) };
    // Capture GetLastError before cleanup can overwrite the thread-local value.
    let error = (result == 0).then(std::io::Error::last_os_error);
    unsafe { LocalFree(descriptor) };
    if let Some(error) = error {
        return Err(error).context("set private state file permissions");
    }
    Ok(())
}

pub fn private_dir(path: &Path) -> Result<()> {
    reject_reparse(path)?;
    fs::create_dir_all(path).context("create private state directory")?;
    restrict_to_owner(path, true)
}

pub fn private_file(path: &Path) -> Result<()> {
    restrict_to_owner(path, false)
}

fn replace(source: &Path, destination: &Path) -> Result<()> {
    reject_reparse(destination.parent().context("missing parent")?)?;
    let source = wide(source.as_os_str());
    let destination = wide(destination.as_os_str());
    let result = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        return Err(std::io::Error::last_os_error()).context("atomically replace state file");
    }
    Ok(())
}

pub fn read<T: DeserializeOwned>(path: &Path) -> Result<T> {
    reject_reparse(path)?;
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(OPEN_REPARSE_POINT)
        .open(path)?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file()
            && metadata.file_attributes() & REPARSE_POINT == 0
            && metadata.len() <= 16 * 1024 * 1024,
        "JSON input must be a bounded regular file"
    );
    let mut bytes = Vec::new();
    file.take(16 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 16 * 1024 * 1024, "JSON input exceeds bound");
    Ok(serde_json::from_slice(&bytes)?)
}

pub fn write<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let parent = path.parent().context("missing parent")?;
    reject_reparse(parent)?;
    fs::create_dir_all(parent)?;
    reject_reparse(parent)?;
    let bytes = serde_json::to_vec_pretty(value)?;
    ensure!(bytes.len() <= 16 * 1024 * 1024, "JSON output exceeds bound");
    let temporary = path.with_extension(format!(
        "pending-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .custom_flags(OPEN_REPARSE_POINT)
        .open(&temporary)?;
    file.write_all(&bytes)
        .context("write temporary state file")?;
    file.sync_all().context("flush temporary state file")?;
    drop(file);
    private_file(&temporary)?;
    replace(&temporary, path)?;
    Ok(())
}

pub struct LockGuard(File);

impl Drop for LockGuard {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.0);
    }
}

pub fn lock(path: &Path) -> Result<LockGuard> {
    let parent = path.parent().context("lock requires parent")?;
    reject_reparse(parent)?;
    fs::create_dir_all(parent)?;
    reject_reparse(path)?;
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .custom_flags(OPEN_REPARSE_POINT)
        .open(path)?;
    ensure!(
        file.metadata()?.file_attributes() & REPARSE_POINT == 0,
        "reparse point lock"
    );
    private_file(path)?;
    if file.try_lock_exclusive().is_err() {
        bail!("another controller owns {}", path.display());
    }
    Ok(LockGuard(file))
}

pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
