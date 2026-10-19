//! Private crash-recovery markers outlive disposable workspace cleanup.
use anyhow::{Result, ensure};
use std::{
    fs,
    path::{Path, PathBuf},
};
use windows_runtime::Container;
pub(super) struct Profile {
    container: Option<Container>,
    marker: PathBuf,
}
impl Profile {
    pub fn create(state: &Path, name: &str) -> Result<Self> {
        let marker = state.join(format!("profile-{name}.json"));
        storage::write(&marker, &name)?;
        let container = Container::create(name)?;
        Ok(Self {
            container: Some(container),
            marker,
        })
    }
    pub fn container(&self) -> &Container {
        self.container.as_ref().expect("profile owns its container")
    }
}
impl Drop for Profile {
    fn drop(&mut self) {
        if self
            .container
            .take()
            .is_some_and(|container| container.close().is_ok())
        {
            let _ = fs::remove_file(&self.marker);
        }
    }
}
pub(super) fn cleanup(state: &Path) -> Result<()> {
    for (count, entry) in fs::read_dir(state)?.enumerate() {
        ensure!(count < 4096, "sandbox recovery inventory exceeds bound");
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with("profile-OutcomeHub.") && name.ends_with(".json") {
            ensure!(
                entry.file_type()?.is_file() && fs::metadata(&path)?.len() < 1024,
                "invalid sandbox recovery marker"
            );
            let profile: String = storage::read(&path)?;
            ensure!(
                name == format!("profile-{profile}.json"),
                "sandbox recovery profile mismatch"
            );
            Container::remove(&profile)?;
            fs::remove_file(path)?;
        } else if name.starts_with("run-") {
            ensure!(
                entry.file_type()?.is_dir() && !entry.file_type()?.is_symlink(),
                "invalid disposable workspace"
            );
            fs::remove_dir_all(path)?;
        }
    }
    Ok(())
}
