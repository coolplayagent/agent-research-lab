//! Windows core starts without external service sockets until a native IPC adapter is configured.
use anyhow::{Result, bail};
use std::path::Path;

pub type Stream = tokio::net::TcpStream;

pub fn endpoint(_path: &Path) -> Result<()> {
    bail!("external service sockets are unavailable on Windows")
}

pub async fn connect(path: &Path) -> Result<Stream> {
    endpoint(path)?;
    unreachable!()
}

pub struct Listener;

impl Listener {
    pub fn bind(path: &Path) -> Result<Self> {
        endpoint(path)?;
        unreachable!()
    }

    pub async fn accept(&self) -> Result<Stream> {
        bail!("external service sockets are unavailable on Windows")
    }
}

#[cfg(test)]
#[path = "windows_tests.rs"]
mod tests;
