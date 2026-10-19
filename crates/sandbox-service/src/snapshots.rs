use anyhow::{Context, Result, ensure};
use std::{fs, path::Path};
pub(super) fn copy(
    source: &Path,
    target: &Path,
    bytes: &mut u64,
    files: &mut usize,
    depth: usize,
) -> Result<()> {
    ensure!(
        depth <= 32 && *files > 0,
        "runtime snapshot file limit exceeded"
    );
    *files -= 1;
    let metadata = fs::symlink_metadata(source)?;
    ensure!(
        !metadata.file_type().is_symlink(),
        "runtime snapshots cannot follow symbolic links"
    );
    if metadata.is_dir() {
        fs::create_dir_all(target)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            copy(
                &entry.path(),
                &target.join(entry.file_name()),
                bytes,
                files,
                depth + 1,
            )?;
        }
    } else {
        ensure!(
            metadata.is_file() && metadata.len() <= *bytes,
            "runtime snapshot byte limit exceeded"
        );
        *bytes -= metadata.len();
        fs::create_dir_all(target.parent().context("runtime parent is missing")?)?;
        fs::copy(source, target)?;
    }
    Ok(())
}
pub(super) fn budget(path: &Path, bytes: &mut u64, files: &mut usize, depth: usize) -> Result<()> {
    ensure!(depth <= 32 && *files > 0, "sandbox file limit exceeded");
    *files -= 1;
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        !metadata.file_type().is_symlink(),
        "sandbox cannot create links outside its workspace"
    );
    if metadata.is_dir() {
        for entry in fs::read_dir(path)? {
            budget(&entry?.path(), bytes, files, depth + 1)?;
        }
    } else {
        ensure!(
            metadata.len() <= *bytes && metadata.len() <= 16 * 1024 * 1024,
            "sandbox file quota exceeded"
        );
        *bytes -= metadata.len();
    }
    Ok(())
}
