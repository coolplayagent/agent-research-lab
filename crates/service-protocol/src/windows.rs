//! Windows core starts without external service sockets until a native IPC adapter is configured.
use anyhow::{Result, bail, ensure};
use std::path::Path;

pub type Stream = tokio::net::TcpStream;

pub fn endpoint(path: &Path) -> Result<()> {
    ensure!(
        path.is_absolute(),
        "service endpoint must be an absolute path"
    );
    Ok(())
}

pub async fn connect(path: &Path) -> Result<Stream> {
    endpoint(path)?;
    bail!("external service sockets are unavailable on Windows")
}

pub struct Listener;

impl Listener {
    pub fn bind(path: &Path) -> Result<Self> {
        endpoint(path)?;
        bail!("external service sockets are unavailable on Windows")
    }

    pub async fn accept(&self) -> Result<Stream> {
        bail!("external service sockets are unavailable on Windows")
    }
}

#[cfg(test)]
#[path = "windows_tests.rs"]
mod tests;
