//! Bounded filesystem transport with anchored directories and no-follow reads.
use crate::{
    limits::{MAX_PROPOSAL_BYTES, MAX_STATE_BYTES},
    protocol::Proposal,
};
use anyhow::{Result, bail, ensure};
use native_effects::fs::{self, File, OpenOptions};
use serde::de::DeserializeOwned;
use std::{
    ffi::CString,
    io::Read,
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    },
    path::{Component, Path, PathBuf},
};

pub(super) enum SlotInput {
    Missing,
    Invalid(&'static str, Option<String>),
    Proposal(Proposal, String),
}

pub(super) fn slot_input(outbox: &Path, slot: usize) -> SlotInput {
    let read = || -> Result<Option<Vec<u8>>> {
        let dir = directory(outbox, false)?;
        let name = CString::new(format!("{slot:02}.json"))?;
        let fd = unsafe {
            libc::openat(
                dir.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            let e = std::io::Error::last_os_error();
            if e.kind() == std::io::ErrorKind::NotFound {
                return Ok(None);
            }
            return Err(e.into());
        }
        let mut file = unsafe { File::from_raw_fd(fd) };
        let meta = file.metadata()?;
        ensure!(
            meta.is_file() && meta.nlink() == 1 && meta.len() <= MAX_PROPOSAL_BYTES as u64,
            "slot must be a bounded regular file without hardlinks"
        );
        let mut data = vec![];
        (&mut file)
            .take(MAX_PROPOSAL_BYTES as u64 + 1)
            .read_to_end(&mut data)?;
        ensure!(data.len() <= MAX_PROPOSAL_BYTES, "slot grew past bound");
        Ok(Some(data))
    };
    match read() {
        Ok(None) => SlotInput::Missing,
        Err(_) => SlotInput::Invalid("unsafe_or_oversized_slot", None),
        Ok(Some(bytes)) => {
            let digest = storage::digest(&bytes);
            match serde_json::from_slice::<Proposal>(&bytes) {
                Ok(p) if p.validate().is_ok() => SlotInput::Proposal(p, digest),
                _ => SlotInput::Invalid("invalid_proposal", Some(digest)),
            }
        }
    }
}

pub(super) fn read_json<T: DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    let mut file = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
    {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    ensure!(
        file.metadata()?.is_file() && file.metadata()?.len() <= MAX_STATE_BYTES as u64,
        "invalid authority file"
    );
    let mut bytes = vec![];
    (&mut file)
        .take(MAX_STATE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= MAX_STATE_BYTES,
        "authority file grew beyond limit"
    );
    Ok(Some(serde_json::from_slice(&bytes)?))
}

/// Anchor each directory component with openat; worker symlink swaps never escape.
pub(super) fn directory(path: &Path, create: bool) -> Result<File> {
    ensure!(path.is_absolute(), "directory path must be absolute");
    let mut dir = File::open("/")?;
    let mut current = PathBuf::from("/");
    for component in path.components() {
        let name = match component {
            Component::RootDir => continue,
            Component::Normal(s) => CString::new(s.as_encoded_bytes())?,
            _ => bail!("non-normal directory component"),
        };
        current.push(component.as_os_str());
        let flags = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC;
        let mut fd = unsafe { libc::openat(dir.as_raw_fd(), name.as_ptr(), flags) };
        if fd < 0
            && create
            && std::io::Error::last_os_error().kind() == std::io::ErrorKind::NotFound
        {
            match fs::mkdir_at(&dir, &current, &name, 0o700) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e.into()),
            }
            fd = unsafe { libc::openat(dir.as_raw_fd(), name.as_ptr(), flags) };
        }
        if fd < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        dir = unsafe { File::from_raw_fd(fd) };
    }
    Ok(dir)
}

pub(super) fn private_directory(path: &Path) -> Result<()> {
    let file = directory(path, true)?;
    ensure!(
        file.metadata()?.uid() == unsafe { libc::geteuid() },
        "board directory is not host-owned"
    );
    fs::set_file_permissions(&file, path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

pub(super) fn remove_view(view: &Path) -> Result<()> {
    match directory(view, false) {
        Ok(dir) => {
            let name = CString::new("board.json")?;
            match fs::unlink_at(&dir, &view.join("board.json"), &name) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
        Err(e)
            if e.downcast_ref::<std::io::Error>()
                .is_some_and(|io| io.kind() == std::io::ErrorKind::NotFound) => {}
        Err(e) => return Err(e),
    }
    match fs::remove_dir(view) {
        Ok(()) => (),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
        Err(e) => return Err(e.into()),
    }
    Ok(())
}
