use anyhow::{Context, Result, ensure};
use fs2::FileExt;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::{
        ffi::OsStrExt,
        fs::{OpenOptionsExt, PermissionsExt},
    },
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
const MAX_FILES: usize = 100_000;
const MAX_BYTES: u64 = 32 * 1024 * 1024 * 1024;
fn paths(root: &Path) -> Result<Vec<PathBuf>> {
    ensure!(
        fs::symlink_metadata(root)?.is_dir(),
        "source must be a real directory"
    );
    let start = Instant::now();
    let mut pending = vec![(root.to_owned(), 0)];
    let mut files = Vec::new();
    let mut count = 0;
    let mut bytes = 0_u64;
    while let Some((dir, depth)) = pending.pop() {
        ensure!(
            depth <= 64 && start.elapsed() < Duration::from_secs(60),
            "state traversal bound exceeded"
        );
        for entry in fs::read_dir(dir)? {
            let path = entry?.path();
            let meta = fs::symlink_metadata(&path)?;
            count += 1;
            ensure!(count <= MAX_FILES, "state entry bound exceeded");
            if meta.is_dir() {
                pending.push((path, depth + 1));
            } else {
                ensure!(
                    meta.is_file(),
                    "state must contain only regular files and directories: {}",
                    path.display()
                );
                ensure!(
                    meta.len() <= 8 * 1024 * 1024 * 1024,
                    "state file exceeds 8 GiB"
                );
                bytes = bytes
                    .checked_add(meta.len())
                    .context("state size overflow")?;
                ensure!(bytes <= MAX_BYTES, "state exceeds 32 GiB");
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}
fn open(path: &Path) -> Result<File> {
    Ok(OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?)
}
fn hash(path: &Path) -> Result<String> {
    let mut file = open(path)?;
    ensure!(file.metadata()?.is_file(), "regular file required");
    let mut digest = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    let mut total = 0_u64;
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        total += n as u64;
        ensure!(total <= 8 * 1024 * 1024 * 1024, "file grew beyond bound");
        digest.update(&buffer[..n]);
    }
    Ok(format!("{:x}", digest.finalize()))
}
pub fn inventory(root: &Path) -> Result<BTreeMap<String, String>> {
    paths(root)?
        .into_iter()
        .map(|p| {
            Ok((
                p.strip_prefix(root)?
                    .to_str()
                    .context("state paths must be UTF-8")?
                    .to_owned(),
                hash(&p)?,
            ))
        })
        .collect()
}
pub struct Offline {
    _locks: Vec<File>,
}
impl Offline {
    pub fn open(root: &Path) -> Result<Self> {
        let files = paths(root)?;
        let mut locks = Vec::new();
        for path in files
            .iter()
            .filter(|p| p.extension().is_some_and(|s| s == "lock"))
        {
            let f = open(path)?;
            f.try_lock_exclusive()
                .with_context(|| format!("source is in use: {}", path.display()))?;
            locks.push(f);
        }
        // Live launches must be stopped, even when their controller no longer owns a lock.
        for path in files.iter().filter(|p| {
            p.parent()
                .and_then(Path::file_name)
                .is_some_and(|s| s == "jobs")
                && p.extension().is_some_and(|s| s == "json")
        }) {
            let job: task::Job = storage::read(path)?;
            if let Some(launch) = job.launch {
                let stat = fs::read_to_string(format!("/proc/{}/stat", launch.pid));
                if let Ok(stat) = stat {
                    let fields: Vec<_> = stat
                        .rsplit_once(") ")
                        .context("invalid process stat")?
                        .1
                        .split_whitespace()
                        .collect();
                    ensure!(
                        fields
                            .get(19)
                            .is_none_or(|start| *start != launch.process_start),
                        "active task process must be stopped"
                    );
                }
            }
        }
        Ok(Self { _locks: locks })
    }
}
pub fn copy(source: &Path, target: &Path, files: &BTreeMap<String, String>) -> Result<()> {
    for (name, expected) in files {
        let from = source.join(name);
        let to = target.join(name);
        fs::create_dir_all(to.parent().unwrap())?;
        fs::set_permissions(to.parent().unwrap(), fs::Permissions::from_mode(0o700))?;
        let mut input = open(&from)?.take(8 * 1024 * 1024 * 1024 + 1);
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600 | (fs::metadata(&from)?.permissions().mode() & 0o100))
            .open(&to)?;
        let n = std::io::copy(&mut input, &mut output)?;
        ensure!(
            n <= 8 * 1024 * 1024 * 1024 && hash(&to)? == *expected,
            "source changed while copying"
        );
        output.flush()?;
        output.sync_all()?;
    }
    Ok(())
}
pub fn sync_tree(root: &Path) -> Result<()> {
    for path in paths(root)? {
        File::open(path)?.sync_all()?;
    }
    fn dirs(path: &Path) -> Result<()> {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                dirs(&entry.path())?;
            }
        }
        File::open(path)?.sync_all()?;
        Ok(())
    }
    dirs(root)
}
pub fn publish(from: &Path, to: &Path) -> Result<()> {
    let from = std::ffi::CString::new(from.as_os_str().as_bytes())?;
    let to = std::ffi::CString::new(to.as_os_str().as_bytes())?;
    // Linux renameat2 guarantees an existing destination is never replaced, including a race.
    let result = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            from.as_ptr(),
            libc::AT_FDCWD,
            to.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    ensure!(
        result == 0,
        "cannot publish migration: {}",
        std::io::Error::last_os_error()
    );
    Ok(())
}
