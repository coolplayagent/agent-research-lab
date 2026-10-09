use anyhow::{Result, bail};
use fs2::FileExt;
use serde::{Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);

pub fn read<T: DeserializeOwned>(p: &Path) -> Result<T> {
    use std::io::Read;
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(p)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > 16 * 1024 * 1024 {
        bail!("JSON input must be a bounded regular file");
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    Ok(serde_json::from_slice(&bytes)?)
}
pub fn write<T: Serialize>(p: &Path, v: &T) -> Result<()> {
    let parent = p
        .parent()
        .ok_or_else(|| anyhow::anyhow!("missing parent"))?;
    fs::create_dir_all(parent)?;
    let temp = p.with_extension(format!(
        "pending-{}-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let mut f = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&temp)?;
    f.write_all(&serde_json::to_vec_pretty(v)?)?;
    f.sync_all()?;
    fs::rename(temp, p)?;
    File::open(parent)?.sync_all()?;
    Ok(())
}
pub struct LockGuard(File);
impl Drop for LockGuard {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.0);
    }
}
pub fn lock(p: &Path) -> Result<LockGuard> {
    fs::create_dir_all(p.parent().unwrap())?;
    let f = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(p)?;
    if f.try_lock_exclusive().is_err() {
        bail!("another controller owns {}", p.display());
    }
    Ok(LockGuard(f))
}
pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reads_reject_symlinks_and_writes_replace_without_following() {
        let d = tempfile::tempdir().unwrap();
        let secret = d.path().join("secret");
        fs::write(&secret, "{\"secret\":1}").unwrap();
        let input = d.path().join("input");
        std::os::unix::fs::symlink(&secret, &input).unwrap();
        assert!(read::<serde_json::Value>(&input).is_err());
        write(&input, &serde_json::json!({"safe":true})).unwrap();
        assert_eq!(fs::read_to_string(secret).unwrap(), "{\"secret\":1}");
        assert_eq!(read::<serde_json::Value>(&input).unwrap()["safe"], true);
    }
}
