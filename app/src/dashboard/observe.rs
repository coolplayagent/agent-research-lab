//! Linux filesystem notifications bridge existing controller files into UI events.
//! Timeouts only permit bounded shutdown; they never trigger a state refresh.
use super::*;
use std::{
    ffi::CString,
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
    path::PathBuf,
    sync::mpsc,
};
use tokio::sync::watch;
const MAX_WATCHES: usize = 8192;
struct Watcher {
    fd: OwnedFd,
    paths: BTreeMap<i32, PathBuf>,
}
impl Watcher {
    fn new() -> Result<Self> {
        // SAFETY: no pointers; OwnedFd takes the unique returned descriptor.
        let fd = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
        ensure!(
            fd >= 0,
            "inotify initialization failed: {}",
            std::io::Error::last_os_error()
        );
        Ok(Self {
            fd: unsafe { OwnedFd::from_raw_fd(fd) },
            paths: BTreeMap::new(),
        })
    }
    fn add(&mut self, path: &Path, depth: usize) -> Result<()> {
        if !path.is_dir() || self.paths.values().any(|p| p == path) {
            return Ok(());
        }
        ensure!(
            self.paths.len() < MAX_WATCHES,
            "observer watch capacity exhausted"
        );
        use std::os::unix::ffi::OsStrExt;
        let name = CString::new(path.as_os_str().as_bytes())?;
        let mask = libc::IN_MODIFY
            | libc::IN_CLOSE_WRITE
            | libc::IN_MOVED_TO
            | libc::IN_CREATE
            | libc::IN_DELETE
            | libc::IN_DELETE_SELF
            | libc::IN_MOVE_SELF
            | libc::IN_ONLYDIR
            | libc::IN_DONT_FOLLOW;
        // SAFETY: valid descriptor, NUL-terminated path, kernel owns no Rust memory.
        let wd = unsafe { libc::inotify_add_watch(self.fd.as_raw_fd(), name.as_ptr(), mask) };
        ensure!(
            wd >= 0,
            "cannot watch controller directory: {}",
            std::io::Error::last_os_error()
        );
        self.paths.insert(wd, path.into());
        if depth > 0 {
            for entry in fs::read_dir(path)? {
                let entry = entry?;
                if entry.file_type()?.is_dir() {
                    self.add(&entry.path(), depth - 1)?;
                }
            }
        }
        Ok(())
    }
    fn receive(&mut self, state: &Path) -> Result<(bool, bool, bool)> {
        let mut poll = libc::pollfd {
            fd: self.fd.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: one live pollfd for the duration of this synchronous call.
        let ready = unsafe { libc::poll(&mut poll, 1, 500) };
        if ready < 0 {
            let e = std::io::Error::last_os_error();
            if e.kind() == std::io::ErrorKind::Interrupted {
                return Ok((false, false, false));
            }
            return Err(e.into());
        }
        if ready == 0 {
            return Ok((false, false, false));
        }
        let mut buffer = [0u8; 64 * 1024];
        // SAFETY: writable byte buffer of the advertised length.
        let count = unsafe {
            libc::read(
                self.fd.as_raw_fd(),
                buffer.as_mut_ptr().cast(),
                buffer.len(),
            )
        };
        if count < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let mut offset = 0;
        let mut full = false;
        let mut session = false;
        let mut overflow = false;
        while offset + std::mem::size_of::<libc::inotify_event>() <= count as usize {
            // SAFETY: checked complete fixed header, possibly unaligned kernel record.
            let event = unsafe {
                std::ptr::read_unaligned(buffer.as_ptr().add(offset).cast::<libc::inotify_event>())
            };
            let start = offset + std::mem::size_of::<libc::inotify_event>();
            let end = start + event.len as usize;
            ensure!(end <= count as usize, "truncated filesystem event");
            let name = &buffer[start..end];
            let name = &name[..name.iter().position(|b| *b == 0).unwrap_or(name.len())];
            offset = end;
            if event.mask & libc::IN_Q_OVERFLOW != 0 {
                overflow = true;
                full = true;
                session = true;
                continue;
            }
            let Some(parent) = self.paths.get(&event.wd).cloned() else {
                continue;
            };
            if event.mask & libc::IN_IGNORED != 0 {
                self.paths.remove(&event.wd);
                continue;
            }
            use std::os::unix::ffi::OsStrExt;
            let path = parent.join(std::ffi::OsStr::from_bytes(name));
            if event.mask & libc::IN_ISDIR != 0
                && event.mask & (libc::IN_CREATE | libc::IN_MOVED_TO) != 0
            {
                // Only approved trees; crystal SQLite and private memory never self-trigger.
                let relative = path.strip_prefix(state).unwrap_or(&path);
                let parts: Vec<_> = relative.components().collect();
                let root = parts
                    .first()
                    .map(|p| p.as_os_str().to_string_lossy())
                    .unwrap_or_default();
                if matches!(
                    root.as_ref(),
                    "jobs" | "people" | "runs" | "communication" | "evolution"
                ) && parts.len() <= 3
                {
                    let depth = if root == "runs"
                        || (root == "communication"
                            && parts.get(1).is_some_and(|p| p.as_os_str() == "views"))
                    {
                        3usize.saturating_sub(parts.len())
                    } else {
                        0
                    };
                    self.add(&path, depth)?;
                    full = true;
                    session = true;
                }
            }
            if name.ends_with(b".lock") || name.starts_with(b".") {
                continue;
            }
            if name == b"stdout.jsonl" {
                session = true;
            } else if name.ends_with(b".json") {
                full = true;
                session = true;
            }
        }
        Ok((full, session, overflow))
    }
}
fn warn(shared: &Shared, updates: &watch::Sender<u64>, message: &str) {
    let mut value = shared.write().unwrap();
    value["error"] = json!(message);
    value["revision"] = json!(value["revision"].as_u64().unwrap_or(0).saturating_add(1));
    drop(value);
    updates.send_modify(|v| *v = v.wrapping_add(1));
}
pub(super) fn run(
    c: &Config,
    shared: &Shared,
    stop: &AtomicBool,
    updates: &watch::Sender<u64>,
) -> Result<()> {
    let mut watcher = Watcher::new()?;
    watcher.add(&c.state_dir, 0)?;
    for (path, depth) in [
        ("jobs", 0),
        ("people", 0),
        ("evolution", 0),
        ("runs", 2),
        ("communication", 0),
        ("communication/views", 1),
    ] {
        watcher.add(&c.state_dir.join(path), depth)?;
    }
    let (full_send, full_recv) = mpsc::sync_channel(1);
    let (session_send, session_recv) = mpsc::sync_channel(1);
    let _ = full_send.try_send(());
    let _ = session_send.try_send(());
    let full_session_send = session_send.clone();
    thread::scope(|scope| {
        scope.spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                match full_recv.recv_timeout(Duration::from_millis(500)) {
                    Ok(()) => {}
                    Err(mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
                match snapshot(c) {
                    Ok(mut value) => {
                        let mut data = shared.write().unwrap();
                        for key in ["sessions", "controller"] {
                            value[key] = data[key].clone();
                        }
                        value["revision"] =
                            json!(data["revision"].as_u64().unwrap_or(0).saturating_add(1));
                        *data = value;
                        drop(data);
                        updates.send_modify(|v| *v = v.wrapping_add(1));
                        let _ = full_session_send.try_send(());
                    }
                    Err(_) => warn(shared, updates, "读取运行状态失败；保留上次记录。"),
                }
            }
        });
        scope.spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                match session_recv.recv_timeout(Duration::from_millis(500)) {
                    Ok(()) => {}
                    Err(mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
                if sessions::update(c, shared) {
                    updates.send_modify(|v| *v = v.wrapping_add(1));
                }
            }
        });
        while !stop.load(Ordering::Relaxed) {
            match watcher.receive(&c.state_dir) {
                Ok((full, session, overflow)) => {
                    if overflow {
                        warn(
                            shared,
                            updates,
                            "文件事件队列溢出，正在重建快照；消息日志可按游标重放。",
                        );
                    }
                    if full {
                        let _ = full_send.try_send(());
                    }
                    if session {
                        let _ = session_send.try_send(());
                    }
                }
                Err(_) => {
                    warn(
                        shared,
                        updates,
                        "文件事件观察中断，请重启观察服务；实时消息服务独立运行。",
                    );
                    break;
                }
            }
        }
        // The outer service owns shutdown. A failed watcher must not block scoped collectors.
        drop(full_send);
        drop(session_send);
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn open_stdout_writes_emit_session_events_before_close() {
        use std::io::Write;
        let path = std::env::temp_dir().join(format!(
            "observer-open-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&path).unwrap();
        let mut file = fs::File::create(path.join("stdout.jsonl")).unwrap();
        let mut watcher = Watcher::new().unwrap();
        watcher.add(&path, 0).unwrap();
        file.write_all(b"{\"type\":\"message\"}\n").unwrap();
        file.flush().unwrap();
        let (full, session, overflow) = watcher.receive(&path).unwrap();
        assert!(session);
        assert!(!full && !overflow);
        drop(file);
        fs::remove_dir_all(path).unwrap();
    }
}
