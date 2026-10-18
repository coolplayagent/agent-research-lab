//! Bounded host commands and filesystem operations used by activation stages.
use anyhow::{Context, Result, ensure};
use std::{
    fs,
    io::Write,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Component, Path},
    time::Duration,
};

pub(super) fn regular(path: &Path, max: u64) -> Result<Vec<u8>> {
    let meta = fs::symlink_metadata(path)?;
    ensure!(
        meta.is_file() && meta.len() <= max,
        "activation input must be a bounded regular file"
    );
    Ok(fs::read(path)?)
}

pub(super) fn relative(path: &Path) -> Result<()> {
    ensure!(
        !path.as_os_str().is_empty()
            && path
                .components()
                .all(|part| matches!(part, Component::Normal(_))),
        "activation artifact path must be relative without traversal"
    );
    Ok(())
}

pub(super) fn command(args: &[String], cwd: &Path, seconds: u64) -> Result<Vec<u8>> {
    let (program, args) = args.split_first().context("activation command is empty")?;
    let output = process::capture(program, args, cwd, Duration::from_secs(seconds))?;
    ensure!(
        output.status.success(),
        "activation command failed: {}",
        String::from_utf8_lossy(&output.stderr)
            .chars()
            .take(1200)
            .collect::<String>()
    );
    Ok(output.stdout)
}

pub(super) fn text(args: &[String], cwd: &Path) -> Result<String> {
    Ok(String::from_utf8(command(args, cwd, 120)?)?
        .trim()
        .to_owned())
}

pub(super) fn write_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .context("activation destination lacks parent")?;
    fs::create_dir_all(parent)?;
    let temporary = path.with_extension(format!("activation-{}", std::process::id()));
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::rename(&temporary, path)?;
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}

pub(super) fn copy_tree(
    source: &Path,
    target: &Path,
    count: &mut usize,
    bytes: &mut u64,
) -> Result<()> {
    fs::create_dir_all(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        *count += 1;
        ensure!(*count <= 16384, "activation package file limit exceeded");
        let metadata = fs::symlink_metadata(entry.path())?;
        ensure!(
            !metadata.file_type().is_symlink(),
            "activation package cannot contain symlinks"
        );
        let destination = target.join(entry.file_name());
        if metadata.is_dir() {
            copy_tree(&entry.path(), &destination, count, bytes)?;
        } else {
            ensure!(
                metadata.is_file(),
                "activation package must contain regular files"
            );
            *bytes += metadata.len();
            ensure!(
                *bytes <= 512 * 1024 * 1024,
                "activation package byte limit exceeded"
            );
            fs::copy(entry.path(), destination)?;
        }
    }
    Ok(())
}

pub(super) fn immutable(path: &Path) -> Result<()> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            immutable(&entry.path())?;
        } else {
            let mode = if entry.metadata()?.permissions().mode() & 0o111 != 0 {
                0o555
            } else {
                0o444
            };
            fs::set_permissions(entry.path(), fs::Permissions::from_mode(mode))?;
        }
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o555))?;
    Ok(())
}
