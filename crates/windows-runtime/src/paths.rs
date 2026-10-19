use anyhow::{Result, ensure};
use std::{
    fs,
    path::{Component, Path, PathBuf, Prefix},
};

/// Use a normal Win32 spelling at third-party CLI boundaries. Some Node releases
/// cannot resolve Rust's canonical verbatim paths. Verify that the alias still
/// identifies the same object before passing it to an external runtime.
pub fn user_path(path: &Path) -> Result<PathBuf> {
    let canonical = fs::canonicalize(path)?;
    let mut components = canonical.components();
    let Some(Component::Prefix(prefix)) = components.next() else {
        return Ok(canonical);
    };
    let mut ordinary = match prefix.kind() {
        Prefix::VerbatimDisk(drive) => PathBuf::from(format!("{}:\\", char::from(drive))),
        Prefix::VerbatimUNC(server, share) => {
            let mut value = std::ffi::OsString::from(r"\\");
            value.push(server);
            value.push(r"\");
            value.push(share);
            PathBuf::from(value)
        }
        _ => return Ok(canonical),
    };
    for component in components {
        if component != Component::RootDir {
            ordinary.push(component.as_os_str());
        }
    }
    ensure!(
        fs::canonicalize(&ordinary)? == canonical,
        "runtime path has no equivalent Win32 spelling"
    );
    Ok(ordinary)
}

#[cfg(test)]
#[path = "paths_tests.rs"]
mod tests;
