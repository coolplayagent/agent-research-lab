//! Bounded, regular-file reads and private directory confinement.
use crate::MAX_FILES;
use anyhow::{Context, Result, ensure};
use native_effects::fs::{self, File, OpenOptions};
use std::{
    io::Read,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Component, Path, PathBuf},
};

pub(super) fn checked_directory(path: &Path, create: bool) -> Result<()> {
    ensure!(path.is_absolute(), "build cache paths must be absolute");
    let mut current = PathBuf::new();
    for component in path.components() {
        ensure!(
            !matches!(component, Component::ParentDir | Component::CurDir),
            "non-canonical cache path"
        );
        current.push(component);
        if create && !current.exists() {
            fs::create_dir(&current)?;
            fs::set_permissions(&current, fs::Permissions::from_mode(0o700))?;
        }
        let metadata = fs::symlink_metadata(&current)?;
        ensure!(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "cache directory must not traverse a symlink"
        );
    }
    Ok(())
}

pub(super) fn regular_file(path: &Path) -> Result<File> {
    checked_directory(path.parent().context("cache file lacks parent")?, false)?;
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    ensure!(
        file.metadata()?.is_file(),
        "cache input must be a regular file"
    );
    Ok(file)
}

pub(super) fn read_bounded(path: &Path, maximum: u64) -> Result<Vec<u8>> {
    let mut file = regular_file(path)?;
    ensure!(
        file.metadata()?.len() <= maximum,
        "cache metadata exceeds byte limit"
    );
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(maximum + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= maximum,
        "cache metadata grew beyond byte limit"
    );
    Ok(bytes)
}

pub(super) fn bounded_entries(path: &Path) -> Result<Vec<fs::DirEntry>> {
    let entries = fs::read_dir(path)?
        .take(MAX_FILES + 1)
        .collect::<std::io::Result<Vec<_>>>()?;
    ensure!(entries.len() <= MAX_FILES, "too many cache entries");
    Ok(entries)
}
