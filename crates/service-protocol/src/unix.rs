use super::*;
use anyhow::Context;
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::path::PathBuf;
pub type Stream = tokio::net::UnixStream;
use tokio::net::UnixListener;

fn owner() -> u32 {
    unsafe { libc::geteuid() }
}
pub fn endpoint(path: &Path) -> Result<()> {
    ensure!(
        path.is_absolute() && path.as_os_str().len() < 104,
        "service socket must be an absolute path shorter than 104 bytes"
    );
    Ok(())
}
pub async fn connect(path: &Path) -> Result<Stream> {
    endpoint(path)?;
    let meta = std::fs::symlink_metadata(path)?;
    ensure!(
        meta.file_type().is_socket()
            && meta.uid() == owner()
            && meta.permissions().mode() & 0o077 == 0,
        "service socket must be private and owned by this user"
    );
    let stream = tokio::time::timeout(Duration::from_secs(5), Stream::connect(path)).await??;
    ensure!(
        stream.peer_cred()?.uid() == owner(),
        "service peer owner mismatch"
    );
    Ok(stream)
}
pub struct Listener {
    pub listener: UnixListener,
    path: PathBuf,
    inode: u64,
}
impl Listener {
    pub fn bind(path: &Path) -> Result<Self> {
        endpoint(path)?;
        let parent = path.parent().context("socket requires parent")?;
        if !parent.exists() {
            std::fs::create_dir_all(parent)?;
            std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
        }
        let meta = std::fs::metadata(parent)?;
        ensure!(
            meta.uid() == owner() && meta.permissions().mode() & 0o077 == 0,
            "socket directory must be private and owned by this user"
        );
        if let Ok(meta) = std::fs::symlink_metadata(path) {
            ensure!(
                meta.file_type().is_socket() && meta.uid() == owner(),
                "refusing to replace a non-socket endpoint"
            );
            ensure!(
                std::os::unix::net::UnixStream::connect(path).is_err(),
                "service endpoint already active"
            );
            std::fs::remove_file(path)?;
        }
        let listener = UnixListener::bind(path)?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        Ok(Self {
            listener,
            path: path.into(),
            inode: std::fs::metadata(path)?.ino(),
        })
    }
    pub async fn accept(&self) -> Result<Stream> {
        let (stream, _) = self.listener.accept().await?;
        ensure!(
            stream.peer_cred()?.uid() == owner(),
            "service peer owner mismatch"
        );
        Ok(stream)
    }
}
impl Drop for Listener {
    fn drop(&mut self) {
        if std::fs::symlink_metadata(&self.path).is_ok_and(|m| m.ino() == self.inode) {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}
