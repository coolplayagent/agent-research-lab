//! Small, versioned local service protocol. No service shares mutable database handles.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{UnixListener, UnixStream},
};
pub const VERSION: u32 = 1;
pub const MAX_FRAME: usize = 8 * 1024 * 1024;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceInfo {
    pub protocol_version: u32,
    pub service_id: String,
    pub kind: contracts::ServiceKind,
    pub capabilities: Vec<String>,
    pub ready: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reply {
    pub protocol_version: u32,
    pub result: serde_json::Value,
    pub error: Option<String>,
}
impl Reply {
    pub fn from_result(result: Result<serde_json::Value>) -> Self {
        match result {
            Ok(result) => Self {
                protocol_version: VERSION,
                result,
                error: None,
            },
            Err(error) => Self {
                protocol_version: VERSION,
                result: serde_json::Value::Null,
                error: Some(error.to_string().chars().take(1000).collect()),
            },
        }
    }
    pub fn decode<T: DeserializeOwned>(self) -> Result<T> {
        ensure!(
            self.protocol_version == VERSION,
            "unsupported service protocol version"
        );
        if let Some(error) = self.error {
            anyhow::bail!("service request failed: {error}");
        }
        Ok(serde_json::from_value(self.result)?)
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope<T> {
    pub protocol_version: u32,
    pub request: T,
}
pub async fn write<T: Serialize>(stream: &mut UnixStream, value: &T) -> Result<()> {
    let bytes = serde_json::to_vec(value)?;
    ensure!(bytes.len() <= MAX_FRAME, "service frame exceeds bound");
    stream.write_u32(bytes.len() as u32).await?;
    stream.write_all(&bytes).await?;
    Ok(())
}
pub async fn read<T: DeserializeOwned>(stream: &mut UnixStream) -> Result<T> {
    let size = stream.read_u32().await? as usize;
    ensure!(size > 0 && size <= MAX_FRAME, "invalid service frame size");
    let mut bytes = vec![0; size];
    stream.read_exact(&mut bytes).await?;
    Ok(serde_json::from_slice(&bytes)?)
}
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
pub async fn connect(path: &Path) -> Result<UnixStream> {
    endpoint(path)?;
    let meta = std::fs::symlink_metadata(path)?;
    ensure!(
        meta.file_type().is_socket()
            && meta.uid() == owner()
            && meta.permissions().mode() & 0o077 == 0,
        "service socket must be private and owned by this user"
    );
    let stream = tokio::time::timeout(Duration::from_secs(5), UnixStream::connect(path)).await??;
    ensure!(
        stream.peer_cred()?.uid() == owner(),
        "service peer owner mismatch"
    );
    Ok(stream)
}
pub async fn open<T: Serialize>(path: &Path, request: &T) -> Result<UnixStream> {
    let mut stream = connect(path).await?;
    tokio::time::timeout(
        Duration::from_secs(5),
        write(
            &mut stream,
            &Envelope {
                protocol_version: VERSION,
                request,
            },
        ),
    )
    .await??;
    Ok(stream)
}
pub async fn call<Q: Serialize, T: DeserializeOwned>(
    path: &Path,
    request: &Q,
    timeout: Duration,
) -> Result<T> {
    tokio::time::timeout(timeout, async {
        let mut stream = open(path, request).await?;
        read::<Reply>(&mut stream).await?.decode()
    })
    .await?
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
    pub async fn accept(&self) -> Result<UnixStream> {
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
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn framed_calls_validate_version_and_private_peer() {
        let root = tempfile::tempdir().unwrap();
        let socket = root.path().join("private/service.sock");
        let listener = Listener::bind(&socket).unwrap();
        let task = tokio::spawn(async move {
            let mut stream = listener.accept().await.unwrap();
            let envelope: Envelope<String> = read(&mut stream).await.unwrap();
            assert_eq!(envelope.request, "probe");
            write(&mut stream, &Reply::from_result(Ok(serde_json::json!(42))))
                .await
                .unwrap();
        });
        assert_eq!(
            call::<_, u32>(&socket, &"probe", Duration::from_secs(2))
                .await
                .unwrap(),
            42
        );
        task.await.unwrap();
        assert!(
            Reply {
                protocol_version: 99,
                result: serde_json::json!(true),
                error: None
            }
            .decode::<bool>()
            .is_err()
        );
    }
}
