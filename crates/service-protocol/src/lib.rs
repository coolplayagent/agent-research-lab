//! Small, versioned local service protocol. No service shares mutable database handles.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{path::Path, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
#[cfg(unix)]
#[path = "unix.rs"]
mod platform;
#[cfg(windows)]
#[path = "windows.rs"]
mod platform;
pub use platform::{Listener, Stream, connect, endpoint};

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
pub async fn write<T: Serialize>(stream: &mut Stream, value: &T) -> Result<()> {
    let bytes = serde_json::to_vec(value)?;
    ensure!(bytes.len() <= MAX_FRAME, "service frame exceeds bound");
    stream.write_u32(bytes.len() as u32).await?;
    stream.write_all(&bytes).await?;
    Ok(())
}
pub async fn read<T: DeserializeOwned>(stream: &mut Stream) -> Result<T> {
    let size = stream.read_u32().await? as usize;
    ensure!(size > 0 && size <= MAX_FRAME, "invalid service frame size");
    let mut bytes = vec![0; size];
    stream.read_exact(&mut bytes).await?;
    Ok(serde_json::from_slice(&bytes)?)
}
pub async fn open<T: Serialize>(path: &Path, request: &T) -> Result<Stream> {
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
#[cfg(all(test, unix))]
#[path = "lib_tests.rs"]
mod tests;
