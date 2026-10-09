use anyhow::{Result, bail};
use fs2::FileExt;
use serde::{Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::Path,
};

pub fn read<T: DeserializeOwned>(p: &Path) -> Result<T> {
    Ok(serde_json::from_slice(&fs::read(p)?)?)
}
pub fn write<T: Serialize>(p: &Path, v: &T) -> Result<()> {
    let parent = p
        .parent()
        .ok_or_else(|| anyhow::anyhow!("missing parent"))?;
    fs::create_dir_all(parent)?;
    let temp = p.with_extension(format!("pending-{}", std::process::id()));
    let mut f = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&temp)?;
    f.write_all(&serde_json::to_vec_pretty(v)?)?;
    f.sync_all()?;
    fs::rename(temp, p)?;
    File::open(parent)?.sync_all()?;
    Ok(())
}
pub fn lock(p: &Path) -> Result<File> {
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
    Ok(f)
}
pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
