//! Packaged local executor services are supervised independently of collaboration storage.
use anyhow::{Result, ensure};
use service_api::Configuration;
#[derive(Default)]
pub(super) struct Managed {
    #[cfg(windows)]
    children: Vec<(contracts::ServiceKind, windows_runtime::Process)>,
}
impl Managed {
    pub fn prepare(&mut self, config: &mut Configuration, state: &std::path::Path) -> Result<()> {
        #[cfg(windows)]
        {
            use anyhow::Context;
            use contracts::ServiceKind;

            let binary = std::env::current_exe()?.with_file_name("outcome-hub-services.exe");
            for kind in [ServiceKind::Sandbox, ServiceKind::Executor] {
                let expected = state.join(format!("private/services/{kind}.sock"));
                let service = config
                    .services
                    .iter_mut()
                    .find(|s| s.id == kind.as_str() && s.kind == kind)
                    .context("local execution service is not configured")?;
                if service.endpoint != expected {
                    continue;
                }
                let service_state = state.join(format!("private/managed-{kind}"));
                if !self.children.iter_mut().any(|(running_kind, child)| {
                    *running_kind == kind && child.poll().is_ok_and(|status| status.is_none())
                }) {
                    ensure!(binary.is_file(), "packaged execution service is missing");
                    storage::private_dir(&service_state)?;
                    let log = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(service_state.join("service.log"))?;
                    storage::private_file(&service_state.join("service.log"))?;
                    let command = vec![
                        binary.to_string_lossy().into_owned(),
                        "--kind".into(),
                        kind.to_string(),
                        "--state-dir".into(),
                        service_state.to_string_lossy().into_owned(),
                        "--socket".into(),
                        service.endpoint.to_string_lossy().into_owned(),
                        "--max-seconds".into(),
                        "43200".into(),
                    ];
                    let null = std::fs::OpenOptions::new()
                        .read(true)
                        .write(true)
                        .open("NUL")?;
                    let child = windows_runtime::Process::spawn(
                        None,
                        &command,
                        &service_state,
                        &std::env::vars().collect::<Vec<_>>(),
                        false,
                        &null,
                        &null,
                        &log,
                    )
                    .context("start packaged execution service")?;
                    self.children.push((kind, child));
                }
                service.enabled = true;
            }
        }
        #[cfg(not(windows))]
        {
            let _ = (self, state);
            ensure!(
                config
                    .services
                    .iter()
                    .any(|s| s.kind == contracts::ServiceKind::Executor && s.enabled),
                "execution service is disabled"
            );
        }
        Ok(())
    }
}
#[cfg(windows)]
impl Drop for Managed {
    fn drop(&mut self) {
        for (_, child) in &mut self.children {
            child.terminate();
        }
    }
}
