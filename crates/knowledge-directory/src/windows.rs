//! Pin every ancestor without delete sharing; reject reparse points on opened handles.
use anyhow::{Result, ensure};
use contracts::{KnowledgeError as Error, KnowledgeErrorCode as Code};
use knowledge_model::Target;
use std::{
    fs::{self, File, OpenOptions},
    os::windows::fs::{MetadataExt, OpenOptionsExt},
    path::{Component, Path, PathBuf, Prefix},
};

const REPARSE: u32 = 0x400;
const OPEN_REPARSE: u32 = 0x0020_0000;
const BACKUP_SEMANTICS: u32 = 0x0200_0000;
#[path = "windows_write.rs"]
mod publication;
pub(super) use publication::write;
pub(super) struct Directory {
    path: PathBuf,
    // Ancestors cannot be renamed or replaced while any traversal is in progress.
    handles: Vec<File>,
}
impl Directory {
    pub fn try_clone(&self) -> Result<Self> {
        Ok(Self {
            path: self.path.clone(),
            handles: self
                .handles
                .iter()
                .map(File::try_clone)
                .collect::<std::io::Result<_>>()?,
        })
    }
}
fn open_directory(path: &Path) -> Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .share_mode(1)
        .custom_flags(OPEN_REPARSE | BACKUP_SEMANTICS)
        .open(path)?;
    let meta = file.metadata()?;
    ensure!(
        !linked(&meta),
        Error::new(Code::SymlinkRejected).parameter("path", path.display())
    );
    ensure!(
        meta.is_dir(),
        Error::new(Code::SpecialFileRejected).parameter("path", path.display())
    );
    Ok(file)
}
pub(super) fn anchored(directory: &Directory) -> PathBuf {
    directory.path.clone()
}
pub(super) fn child(root: &Directory, path: &Path, create: bool) -> Result<Directory> {
    let mut current = root.try_clone()?;
    for component in path.components() {
        let Component::Normal(name) = component else {
            anyhow::bail!("knowledge_path_escape")
        };
        current.path.push(name);
        if create {
            match fs::create_dir(&current.path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e.into()),
            }
        }
        current.handles.push(open_directory(&current.path)?);
    }
    Ok(current)
}
pub(super) fn root(target: &Target) -> Result<Directory> {
    target.validate()?;
    let mut current = Directory {
        path: PathBuf::new(),
        handles: Vec::new(),
    };
    for component in target.path.components() {
        match component {
            Component::Prefix(prefix) => {
                ensure!(
                    matches!(
                        prefix.kind(),
                        Prefix::Disk(_)
                            | Prefix::VerbatimDisk(_)
                            | Prefix::UNC(_, _)
                            | Prefix::VerbatimUNC(_, _)
                    ),
                    "knowledge_path_escape"
                );
                current.path.push(prefix.as_os_str());
            }
            Component::RootDir => {
                current.path.push(component.as_os_str());
                current.handles.push(open_directory(&current.path)?);
            }
            Component::Normal(name) => current = child(&current, Path::new(name), false)?,
            _ => anyhow::bail!("knowledge_path_escape"),
        }
    }
    ensure!(
        !current.handles.is_empty(),
        "knowledge_absolute_path_required"
    );
    if target.content_root == Path::new(".") {
        Ok(current)
    } else {
        child(&current, &target.content_root, false)
    }
}
pub(super) fn open_file(path: &Path) -> Result<File> {
    // Deny write/delete sharing until the bounded read and identity checks finish.
    let file = OpenOptions::new()
        .read(true)
        .share_mode(1)
        .custom_flags(OPEN_REPARSE)
        .open(path)?;
    ensure!(
        !linked(&file.metadata()?),
        Error::new(Code::SymlinkRejected).parameter("path", path.display())
    );
    Ok(file)
}
pub(super) fn linked(meta: &fs::Metadata) -> bool {
    meta.file_attributes() & REPARSE != 0
}
pub(super) fn identity(meta: &fs::Metadata) -> (u64, u64, u64, u32) {
    (
        meta.file_size(),
        meta.creation_time(),
        meta.last_write_time(),
        meta.file_attributes(),
    )
}
pub(super) fn lock(path: &Path) -> Result<File> {
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .share_mode(3)
        .custom_flags(OPEN_REPARSE)
        .open(path)?;
    ensure!(
        !linked(&file.metadata()?),
        Error::new(Code::SymlinkRejected).parameter("path", path.display())
    );
    ensure!(file.try_lock().is_ok(), "knowledge_target_busy");
    Ok(file)
}

#[cfg(test)]
#[path = "windows_tests.rs"]
mod tests;
