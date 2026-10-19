//! Suspended AppContainer startup: attach the job before any application code runs.
use crate::{Container, Job, security::wide};
use anyhow::{Context, Result, ensure};
use std::{ffi::OsStr, fs::File, os::windows::io::AsRawHandle, path::Path, ptr};
use windows_sys::Win32::{
    Foundation::*,
    Security::*,
    System::{SystemServices::SE_GROUP_ENABLED, Threading::*},
};

pub struct Process {
    handle: HANDLE,
    job: Job,
}
// Polling and termination use thread-safe kernel handles; ownership is unique.
unsafe impl Send for Process {}
impl Process {
    #[allow(clippy::too_many_arguments)]
    pub fn spawn(
        container: Option<&Container>,
        command: &[String],
        cwd: &Path,
        environment: &[(String, String)],
        network: bool,
        stdin: &File,
        stdout: &File,
        stderr: &File,
    ) -> Result<Self> {
        ensure!(
            !command.is_empty() && Path::new(&command[0]).is_absolute(),
            "sandbox program must be absolute"
        );
        let mut line = wide(OsStr::new(
            &command
                .iter()
                .map(|s| quote(s))
                .collect::<Vec<_>>()
                .join(" "),
        ));
        ensure!(
            line.len() < 32767,
            "sandbox command line exceeds Windows bound"
        );
        let mut env = environment.to_vec();
        env.sort_by_key(|(name, _)| name.to_uppercase());
        let mut block = Vec::new();
        for (name, value) in env {
            ensure!(
                !name.is_empty() && !name.contains(['=', '\0']) && !value.contains('\0'),
                "invalid sandbox environment"
            );
            block.extend(wide(OsStr::new(&format!("{name}={value}"))));
        }
        block.push(0);
        if block.len() == 1 {
            block.push(0);
        }
        let handles = [
            stdin.as_raw_handle(),
            stdout.as_raw_handle(),
            stderr.as_raw_handle(),
        ];
        for handle in handles {
            if unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT) }
                == 0
            {
                return Err(std::io::Error::last_os_error())
                    .context("mark sandbox standard handle inheritable");
            }
        }
        let result = Self::create(
            container, command, cwd, network, &handles, &mut line, &block,
        );
        for handle in handles {
            unsafe {
                SetHandleInformation(handle, HANDLE_FLAG_INHERIT, 0);
            }
        }
        result
    }
    fn create(
        container: Option<&Container>,
        command: &[String],
        cwd: &Path,
        network: bool,
        handles: &[HANDLE; 3],
        line: &mut [u16],
        block: &[u16],
    ) -> Result<Self> {
        let attributes = Attributes::new(if container.is_some() { 2 } else { 1 })?;
        let mut capability_sid = [0usize; 16];
        let mut sid_size = size_of_val(&capability_sid) as u32;
        if network
            && unsafe {
                CreateWellKnownSid(
                    WinCapabilityInternetClientSid,
                    ptr::null_mut(),
                    capability_sid.as_mut_ptr().cast(),
                    &mut sid_size,
                )
            } == 0
        {
            return Err(std::io::Error::last_os_error()).context("create network capability");
        }
        let capability = SID_AND_ATTRIBUTES {
            Sid: capability_sid.as_mut_ptr().cast(),
            Attributes: SE_GROUP_ENABLED as u32,
        };
        let security = SECURITY_CAPABILITIES {
            AppContainerSid: container.map_or(ptr::null_mut(), Container::sid),
            Capabilities: if network {
                &capability as *const _ as *mut _
            } else {
                ptr::null_mut()
            },
            CapabilityCount: u32::from(network),
            Reserved: 0,
        };
        if container.is_some() {
            attributes.update(PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES, &security)?;
        }
        let mut inherited = handles.to_vec();
        inherited.sort();
        inherited.dedup();
        attributes.update_slice(PROC_THREAD_ATTRIBUTE_HANDLE_LIST, &inherited)?;
        let mut startup = STARTUPINFOEXW::default();
        startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
        startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
        startup.StartupInfo.hStdInput = handles[0];
        startup.StartupInfo.hStdOutput = handles[1];
        startup.StartupInfo.hStdError = handles[2];
        startup.lpAttributeList = attributes.pointer();
        let mut info = PROCESS_INFORMATION::default();
        let job = Job::new()?;
        let result = unsafe {
            CreateProcessW(
                wide(OsStr::new(&command[0])).as_ptr(),
                line.as_mut_ptr(),
                ptr::null(),
                ptr::null(),
                1,
                EXTENDED_STARTUPINFO_PRESENT
                    | CREATE_UNICODE_ENVIRONMENT
                    | CREATE_SUSPENDED
                    | CREATE_NO_WINDOW,
                block.as_ptr().cast(),
                wide(cwd.as_os_str()).as_ptr(),
                &startup.StartupInfo,
                &mut info,
            )
        };
        if result == 0 {
            return Err(std::io::Error::last_os_error()).context("launch AppContainer process");
        }
        let attached = job.assign(info.hProcess).and_then(|()| {
            if unsafe { ResumeThread(info.hThread) } == u32::MAX {
                return Err(std::io::Error::last_os_error()).context("resume sandbox process");
            }
            Ok(())
        });
        unsafe {
            CloseHandle(info.hThread);
        }
        if let Err(error) = attached {
            unsafe {
                TerminateProcess(info.hProcess, 1);
                WaitForSingleObject(info.hProcess, 5000);
                CloseHandle(info.hProcess);
            }
            return Err(error);
        }
        Ok(Self {
            handle: info.hProcess,
            job,
        })
    }
    pub fn terminate(&self) {
        self.job.terminate();
    }
    pub fn poll(&self) -> Result<Option<u32>> {
        let ready = unsafe { WaitForSingleObject(self.handle, 0) };
        if ready == WAIT_TIMEOUT {
            return Ok(None);
        }
        if ready == WAIT_FAILED {
            return Err(std::io::Error::last_os_error()).context("poll sandbox process");
        }
        let mut code = 0;
        if unsafe { GetExitCodeProcess(self.handle, &mut code) } == 0 {
            return Err(std::io::Error::last_os_error()).context("read sandbox exit status");
        }
        Ok(Some(code))
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        self.job.terminate();
        unsafe {
            WaitForSingleObject(self.handle, 5000);
            CloseHandle(self.handle);
        }
    }
}
struct Attributes(Vec<usize>);
impl Attributes {
    fn new(count: u32) -> Result<Self> {
        let mut bytes = 0;
        unsafe {
            InitializeProcThreadAttributeList(ptr::null_mut(), count, 0, &mut bytes);
        }
        ensure!(bytes > 0 && bytes < 65536, "invalid process attribute size");
        let mut storage = vec![0; bytes.div_ceil(size_of::<usize>())];
        if unsafe {
            InitializeProcThreadAttributeList(storage.as_mut_ptr().cast(), count, 0, &mut bytes)
        } == 0
        {
            return Err(std::io::Error::last_os_error())
                .context("initialize sandbox process attributes");
        }
        Ok(Self(storage))
    }
    fn pointer(&self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
        self.0.as_ptr() as *mut _
    }
    fn update<T>(&self, key: u32, value: &T) -> Result<()> {
        if unsafe {
            UpdateProcThreadAttribute(
                self.pointer(),
                0,
                key as usize,
                (value as *const T).cast(),
                size_of::<T>(),
                ptr::null_mut(),
                ptr::null(),
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error()).context("set sandbox process attribute");
        }
        Ok(())
    }
    fn update_slice<T>(&self, key: u32, values: &[T]) -> Result<()> {
        if unsafe {
            UpdateProcThreadAttribute(
                self.pointer(),
                0,
                key as usize,
                values.as_ptr().cast(),
                size_of_val(values),
                ptr::null_mut(),
                ptr::null(),
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error()).context("set sandbox handle inheritance");
        }
        Ok(())
    }
}
impl Drop for Attributes {
    fn drop(&mut self) {
        unsafe {
            DeleteProcThreadAttributeList(self.pointer());
        }
    }
}
fn quote(argument: &str) -> String {
    if !argument.is_empty() && !argument.contains([' ', '\t', '"']) {
        return argument.into();
    }
    let mut result = String::from("\"");
    let mut slashes = 0;
    for character in argument.chars() {
        if character == '\\' {
            slashes += 1;
            continue;
        }
        result.extend(std::iter::repeat_n(
            '\\',
            if character == '"' {
                slashes * 2 + 1
            } else {
                slashes
            },
        ));
        slashes = 0;
        result.push(character);
    }
    result.extend(std::iter::repeat_n('\\', slashes * 2));
    result.push('"');
    result
}
#[cfg(test)]
#[path = "launch_tests.rs"]
mod tests;
