use anyhow::{Context, Result, ensure};
use std::{ffi::OsStr, os::windows::ffi::OsStrExt, ptr};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, LocalFree},
    Security::{Authorization::*, GetTokenInformation, TOKEN_QUERY, TOKEN_USER, TokenUser},
    System::Threading::{
        GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
    },
};

pub fn wide(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(Some(0)).collect()
}

pub fn owner_sid() -> Result<String> {
    process_sid(unsafe { GetCurrentProcess() })
}
fn process_sid(process: HANDLE) -> Result<String> {
    let mut token = ptr::null_mut();
    if unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) } == 0 {
        return Err(std::io::Error::last_os_error()).context("open process identity");
    }
    let result = (|| {
        let mut size = 0;
        unsafe {
            GetTokenInformation(token, TokenUser, ptr::null_mut(), 0, &mut size);
        }
        ensure!(size > 0 && size < 65536, "invalid process identity size");
        let mut buffer = vec![0usize; (size as usize).div_ceil(size_of::<usize>())];
        if unsafe {
            GetTokenInformation(
                token,
                TokenUser,
                buffer.as_mut_ptr().cast(),
                size,
                &mut size,
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error()).context("read process identity");
        }
        let user = unsafe { &*buffer.as_ptr().cast::<TOKEN_USER>() };
        sid_string(user.User.Sid)
    })();
    unsafe {
        CloseHandle(token);
    }
    result
}
pub(crate) fn sid_string(sid: windows_sys::Win32::Security::PSID) -> Result<String> {
    let mut encoded = ptr::null_mut();
    if unsafe { ConvertSidToStringSidW(sid, &mut encoded) } == 0 {
        return Err(std::io::Error::last_os_error()).context("encode process identity");
    }
    let mut size = 0;
    while unsafe { *encoded.add(size) } != 0 {
        size += 1;
    }
    let result = String::from_utf16(unsafe { std::slice::from_raw_parts(encoded, size) });
    unsafe {
        LocalFree(encoded.cast());
    }
    Ok(result?)
}
pub fn verify_process_owner(pid: u32) -> Result<()> {
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    ensure!(!process.is_null(), "service peer identity is unavailable");
    let actual = process_sid(process);
    unsafe {
        CloseHandle(process);
    }
    ensure!(actual? == owner_sid()?, "service peer owner mismatch");
    Ok(())
}
pub struct SecurityDescriptor(windows_sys::Win32::Security::PSECURITY_DESCRIPTOR);
impl SecurityDescriptor {
    pub fn from_sddl(sddl: &str) -> Result<Self> {
        let mut descriptor = ptr::null_mut();
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                wide(OsStr::new(sddl)).as_ptr(),
                1,
                &mut descriptor,
                ptr::null_mut(),
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error())
                .context("create private security descriptor");
        }
        Ok(Self(descriptor))
    }
    pub fn private() -> Result<Self> {
        Self::from_sddl(&format!("D:P(A;;GA;;;{})(A;;GA;;;SY)", owner_sid()?))
    }
    pub fn attributes(&self) -> windows_sys::Win32::Security::SECURITY_ATTRIBUTES {
        windows_sys::Win32::Security::SECURITY_ATTRIBUTES {
            nLength: size_of::<windows_sys::Win32::Security::SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.0,
            bInheritHandle: 0,
        }
    }
    pub fn pointer(&self) -> windows_sys::Win32::Security::PSECURITY_DESCRIPTOR {
        self.0
    }
}
impl Drop for SecurityDescriptor {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.0);
        }
    }
}
