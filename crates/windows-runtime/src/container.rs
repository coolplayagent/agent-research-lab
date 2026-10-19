use crate::security::{SecurityDescriptor, owner_sid, sid_string, wide};
use anyhow::{Context, Result, ensure};
use std::{ffi::OsStr, path::Path, ptr};
use windows_sys::Win32::Security::{Isolation::*, *};

pub struct Container {
    name: Vec<u16>,
    sid: PSID,
}
// The profile and SID remain owned until all associated process handles are closed.
unsafe impl Send for Container {}
unsafe impl Sync for Container {}
impl Container {
    pub fn remove(name: &str) -> Result<()> {
        ensure!(
            name.starts_with("OutcomeHub.")
                && name.len() <= 64
                && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.'),
            "invalid sandbox profile name"
        );
        let result = unsafe { DeleteAppContainerProfile(wide(OsStr::new(name)).as_ptr()) };
        ensure!(
            result >= 0 || [0x80070002u32, 0x80070490].contains(&(result as u32)),
            "remove AppContainer profile failed (HRESULT {result:#x})"
        );
        Ok(())
    }
    pub fn close(mut self) -> Result<()> {
        let result = unsafe { DeleteAppContainerProfile(self.name.as_ptr()) };
        ensure!(
            result >= 0,
            "remove AppContainer profile failed (HRESULT {result:#x})"
        );
        self.name.clear();
        Ok(())
    }
    pub fn create(name: &str) -> Result<Self> {
        ensure!(
            name.starts_with("OutcomeHub.")
                && name.len() <= 64
                && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.'),
            "invalid sandbox profile name"
        );
        let name = wide(OsStr::new(name));
        let mut sid = ptr::null_mut();
        let result = unsafe {
            CreateAppContainerProfile(
                name.as_ptr(),
                name.as_ptr(),
                name.as_ptr(),
                ptr::null(),
                0,
                &mut sid,
            )
        };
        ensure!(
            result >= 0,
            "create AppContainer profile failed (HRESULT {result:#x})"
        );
        Ok(Self { name, sid })
    }
    pub(crate) fn sid(&self) -> PSID {
        self.sid
    }
    /// Apply permissions only to a disposable snapshot owned by the caller.
    pub fn grant(
        &self,
        path: &Path,
        writable: bool,
        check: &impl Fn() -> Result<()>,
    ) -> Result<()> {
        check()?;
        ensure!(
            path.is_absolute() && !std::fs::symlink_metadata(path)?.file_type().is_symlink(),
            "sandbox ACL target must be an absolute snapshot"
        );
        let sid = sid_string(self.sid)?;
        let access = if writable { "GA" } else { "GRGX" };
        let descriptor = SecurityDescriptor::from_sddl(&format!(
            "D:P(A;OICI;GA;;;{})(A;OICI;GA;;;SY)(A;OICI;{access};;;{sid})S:(ML;OICI;NW;;;LW)",
            owner_sid()?
        ))?;
        if unsafe {
            SetFileSecurityW(
                wide(path.as_os_str()).as_ptr(),
                DACL_SECURITY_INFORMATION | LABEL_SECURITY_INFORMATION,
                descriptor.pointer(),
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error())
                .context("grant AppContainer snapshot access");
        }
        if path.is_dir() {
            // Snapshot children may have protected ACLs from private configuration copies.
            for entry in std::fs::read_dir(path)? {
                self.grant(&entry?.path(), writable, check)?;
            }
        }
        Ok(())
    }
}
impl Drop for Container {
    fn drop(&mut self) {
        unsafe {
            FreeSid(self.sid);
            if !self.name.is_empty() {
                DeleteAppContainerProfile(self.name.as_ptr());
            }
        }
    }
}
