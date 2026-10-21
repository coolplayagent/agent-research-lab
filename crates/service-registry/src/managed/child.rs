//! Platform process handles with bounded lifetime, private logs and owned cleanup.
use anyhow::Result;
use service_api::Service;
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
static SEQUENCE: AtomicU64 = AtomicU64::new(0);
pub(super) struct Child {
    #[cfg(unix)]
    process: Option<process::Process>,
    #[cfg(windows)]
    process: Option<windows_runtime::Process>,
    logs: PathBuf,
}
impl Child {
    pub fn spawn(binary: &Path, service: &Service, state: &Path) -> Result<Self> {
        let directory = state.join(format!("private/managed-{}", service.id));
        storage::private_dir(&directory)?;
        let logs = directory.join(format!(
            "launch-{}-{}-{}",
            std::process::id(),
            crate::bindings::now(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        storage::private_dir(&logs)?;
        let mut child = Self {
            process: None,
            logs,
        };
        let args = vec![
            "--kind".into(),
            service.kind.to_string(),
            "--state-dir".into(),
            directory.to_string_lossy().into_owned(),
            "--socket".into(),
            service.endpoint.to_string_lossy().into_owned(),
            "--max-seconds".into(),
            "43200".into(),
        ];
        #[cfg(unix)]
        let process = process::Process::spawn(
            &binary.to_string_lossy(),
            &args,
            &directory,
            &child.logs,
            std::time::Duration::from_secs(43200),
            &[],
            None,
        )?;
        #[cfg(windows)]
        let process = {
            let log_path = child.logs.join("stderr.log");
            let log = std::fs::File::create(&log_path)?;
            storage::private_file(&log_path)?;
            let null = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open("NUL")?;
            let mut command = vec![binary.to_string_lossy().into_owned()];
            command.extend(args);
            windows_runtime::Process::spawn(
                None,
                &command,
                &directory,
                &std::env::vars().collect::<Vec<_>>(),
                false,
                &null,
                &null,
                &log,
            )?
        };
        child.process = Some(process);
        Ok(child)
    }
    pub fn running(&mut self) -> bool {
        if std::fs::metadata(self.logs.join("stderr.log")).is_ok_and(|m| m.len() > 16 * 1024 * 1024)
        {
            return false;
        }
        self.process
            .as_mut()
            .is_some_and(|child| child.poll().is_ok_and(|status| status.is_none()))
    }
}
impl Drop for Child {
    fn drop(&mut self) {
        self.process.take();
        let _ = std::fs::remove_dir_all(&self.logs);
    }
}
#[cfg(test)]
#[path = "child_tests.rs"]
mod tests;
