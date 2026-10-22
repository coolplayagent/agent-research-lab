//! Linux descriptor-relative filesystem access.
use anyhow::{Result, ensure};
use knowledge_model::Target;
use std::{
    fs::{self, File, OpenOptions},
    os::{
        fd::AsRawFd,
        unix::fs::{MetadataExt, OpenOptionsExt},
    },
    path::{Component, Path, PathBuf},
};
pub(super) type Directory = File;
pub(super) fn write<T: serde::Serialize>(parent: &Directory, name: &Path, value: &T) -> Result<()> {
    storage::write(&anchored(parent).join(name), value)
}
pub(super) fn open_directory(path: &Path) -> Result<File> {
    Ok(OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
        .open(path)?)
}
pub(super) fn anchored(file: &File) -> PathBuf {
    PathBuf::from(format!("/proc/self/fd/{}", file.as_raw_fd()))
}
pub(super) fn child(root: &File, path: &Path, create: bool) -> Result<File> {
    let mut current = root.try_clone()?;
    for c in path.components() {
        let Component::Normal(name) = c else {
            anyhow::bail!("knowledge_path_escape")
        };
        let next = anchored(&current).join(name);
        if create && !next.exists() {
            match fs::create_dir(&next) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e.into()),
            }
        }
        current = open_directory(&next)?;
    }
    Ok(current)
}
pub(super) fn root(target: &Target) -> Result<File> {
    target.validate()?;
    let base = child(
        &open_directory(Path::new("/"))?,
        target.path.strip_prefix("/")?,
        false,
    )?;
    if target.content_root == Path::new(".") {
        Ok(base)
    } else {
        child(&base, &target.content_root, false)
    }
}

pub(super) fn open_file(path: &Path) -> Result<File> {
    Ok(OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?)
}
pub(super) fn linked(meta: &fs::Metadata) -> bool {
    meta.file_type().is_symlink()
}
pub(super) fn identity(meta: &fs::Metadata) -> (u64, u64, u64, i64, i64, i64, i64) {
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
pub(super) fn lock(path: &Path) -> Result<File> {
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    ensure!(file.try_lock().is_ok(), "knowledge_target_busy");
    Ok(file)
}
