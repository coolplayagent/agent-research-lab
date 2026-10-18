//! Deterministic bounded skill/runtime hashing and path component checks.
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
};

/// Canonical manifest contract: sorted relative POSIX UTF-8 file paths; hash
/// path || NUL || raw file SHA256 (32 bytes) || newline for every regular file.
/// Empty directories are ignored. Symlinks and special files are rejected.
pub fn skill_tree_digest(root: &Path) -> Result<String> {
    ensure!(root.is_absolute(), "skill tree path must be absolute");
    reject_symlink_components(root)?;
    ensure!(
        fs::metadata(root)?.is_dir(),
        "skill root is not a directory"
    );
    let mut pending = vec![root.to_owned()];
    let mut files = vec![];
    let mut entries = 0usize;
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory)? {
            let path = entry?.path();
            entries += 1;
            ensure!(entries <= 10000, "skill tree exceeds 10000 entries");
            let metadata = fs::symlink_metadata(&path)?;
            ensure!(
                !metadata.file_type().is_symlink(),
                "skill tree contains a symlink"
            );
            if metadata.is_dir() {
                pending.push(path);
            } else {
                ensure!(metadata.is_file(), "skill tree contains a special file");
                let relative = path
                    .strip_prefix(root)?
                    .components()
                    .map(|part| part.as_os_str().to_str().context("skill path is not UTF-8"))
                    .collect::<Result<Vec<_>>>()?
                    .join("/");
                files.push((relative, path));
            }
        }
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    let mut tree = Sha256::new();
    let mut bytes = 0_u64;
    for (relative, file) in files {
        let (digest, size) = regular_file_digest(&file)?;
        bytes = bytes
            .checked_add(size)
            .context("skill tree size overflow")?;
        ensure!(bytes <= 1024 * 1024 * 1024, "skill tree exceeds 1 GiB");
        tree.update(relative.as_bytes());
        tree.update([0]);
        tree.update(digest);
        tree.update(b"\n");
    }
    Ok(hex(&tree.finalize()))
}

pub(super) fn regular_file_digest(path: &Path) -> Result<([u8; 32], u64)> {
    reject_symlink_components(path)?;
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file() && metadata.len() <= 256 * 1024 * 1024,
        "runtime/skill file must be regular and at most 256 MiB"
    );
    let mut file = fs::File::open(path)?;
    let mut sha = Sha256::new();
    let mut buffer = [0_u8; 65536];
    let mut size = 0_u64;
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        size += count as u64;
        ensure!(size <= 256 * 1024 * 1024, "file grew beyond bound");
        sha.update(&buffer[..count]);
    }
    ensure!(size == metadata.len(), "file changed while hashing");
    Ok((sha.finalize().into(), size))
}

pub(super) fn reject_symlink_components(path: &Path) -> Result<()> {
    ensure!(path.is_absolute(), "freshness paths must be absolute");
    let mut current = PathBuf::new();
    for component in path.components() {
        ensure!(
            !matches!(component, Component::ParentDir),
            "parent path traversal is not allowed"
        );
        current.push(component.as_os_str());
        ensure!(
            !fs::symlink_metadata(&current)?.file_type().is_symlink(),
            "freshness path traverses a symlink"
        );
    }
    Ok(())
}

pub(super) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
#[path = "files_tests.rs"]
mod tests;
