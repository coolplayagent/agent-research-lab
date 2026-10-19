//! Private local named pipes retain the existing path-valued endpoint contract.
use anyhow::{Context, Result, ensure};
use std::{
    io,
    os::windows::io::AsRawHandle,
    path::Path,
    pin::Pin,
    task::{Context as TaskContext, Poll},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::windows::named_pipe::{ClientOptions, NamedPipeClient, NamedPipeServer, ServerOptions},
    sync::Mutex,
};
use windows_runtime::{SecurityDescriptor, owner_sid, verify_process_owner};
use windows_sys::Win32::System::Pipes::{GetNamedPipeClientProcessId, GetNamedPipeServerProcessId};

pub fn endpoint(path: &Path) -> Result<()> {
    ensure!(
        path.is_absolute()
            && path
                .components()
                .all(|p| !matches!(p, std::path::Component::ParentDir)),
        "service endpoint must be an absolute normalized path"
    );
    Ok(())
}
fn name(path: &Path) -> Result<String> {
    endpoint(path)?;
    let normalized = path
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .replace('/', "\\")
        .to_lowercase();
    Ok(format!(
        r"\\.\pipe\OutcomeHub-{}",
        windows_runtime::endpoint_digest(&format!("{}:{normalized}", owner_sid()?))
    ))
}
pub enum Stream {
    Client(NamedPipeClient),
    Server(NamedPipeServer),
}
impl Stream {
    pub async fn readable(&self) -> io::Result<()> {
        match self {
            Self::Client(v) => v.readable().await,
            Self::Server(v) => v.readable().await,
        }
    }
    pub fn try_read(&self, data: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Client(v) => v.try_read(data),
            Self::Server(v) => v.try_read(data),
        }
    }
}
impl AsyncRead for Stream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Client(v) => Pin::new(v).poll_read(cx, buf),
            Self::Server(v) => Pin::new(v).poll_read(cx, buf),
        }
    }
}
impl AsyncWrite for Stream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            Self::Client(v) => Pin::new(v).poll_write(cx, buf),
            Self::Server(v) => Pin::new(v).poll_write(cx, buf),
        }
    }
    fn poll_flush(self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Client(v) => Pin::new(v).poll_flush(cx),
            Self::Server(v) => Pin::new(v).poll_flush(cx),
        }
    }
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Client(v) => Pin::new(v).poll_shutdown(cx),
            Self::Server(v) => Pin::new(v).poll_shutdown(cx),
        }
    }
}
pub async fn connect(path: &Path) -> Result<Stream> {
    let name = name(path)?;
    let pipe = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match ClientOptions::new().open(&name) {
                Ok(pipe) => return Ok::<_, io::Error>(pipe),
                Err(error) if error.raw_os_error() == Some(231) => {
                    tokio::time::sleep(Duration::from_millis(20)).await
                }
                Err(error) => return Err(error),
            }
        }
    })
    .await??;
    let mut pid = 0;
    ensure!(
        unsafe { GetNamedPipeServerProcessId(pipe.as_raw_handle(), &mut pid) } != 0,
        "service peer identity unavailable"
    );
    verify_process_owner(pid)?;
    Ok(Stream::Client(pipe))
}
pub struct Listener {
    name: String,
    next: Mutex<NamedPipeServer>,
}
fn instance(name: &str, first: bool) -> Result<NamedPipeServer> {
    let security = SecurityDescriptor::private()?;
    let mut attributes = security.attributes();
    unsafe {
        ServerOptions::new()
            .first_pipe_instance(first)
            .reject_remote_clients(true)
            .create_with_security_attributes_raw(
                name,
                (&mut attributes as *mut windows_sys::Win32::Security::SECURITY_ATTRIBUTES).cast(),
            )
    }
    .context("create private service pipe")
}
impl Listener {
    pub fn bind(path: &Path) -> Result<Self> {
        let name = name(path)?;
        let next = Mutex::new(instance(&name, true)?);
        Ok(Self { name, next })
    }
    pub async fn accept(&self) -> Result<Stream> {
        let mut next = self.next.lock().await;
        next.connect().await?;
        let replacement = instance(&self.name, false)?;
        let pipe = std::mem::replace(&mut *next, replacement);
        let mut pid = 0;
        ensure!(
            unsafe { GetNamedPipeClientProcessId(pipe.as_raw_handle(), &mut pid) } != 0,
            "service client identity unavailable"
        );
        verify_process_owner(pid)?;
        Ok(Stream::Server(pipe))
    }
}
#[cfg(test)]
#[path = "windows_tests.rs"]
mod tests;
