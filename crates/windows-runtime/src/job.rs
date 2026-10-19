use anyhow::{Context, Result};
use std::ptr;
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE},
    System::JobObjects::*,
};

pub struct Job(HANDLE);
// A job handle can be closed or assigned from any thread; it is never inherited.
unsafe impl Send for Job {}
unsafe impl Sync for Job {}
impl Job {
    pub fn new() -> Result<Self> {
        let handle = unsafe { CreateJobObjectW(ptr::null(), ptr::null()) };
        if handle.is_null() {
            return Err(std::io::Error::last_os_error()).context("create process job");
        }
        let job = Self(handle);
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
            | JOB_OBJECT_LIMIT_ACTIVE_PROCESS
            | JOB_OBJECT_LIMIT_JOB_MEMORY;
        limits.BasicLimitInformation.ActiveProcessLimit = 64;
        limits.JobMemoryLimit = 4 * 1024 * 1024 * 1024;
        if unsafe {
            SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                size_of_val(&limits) as u32,
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error()).context("configure process job limits");
        }
        Ok(job)
    }
    pub(crate) fn assign(&self, process: HANDLE) -> Result<()> {
        if unsafe { AssignProcessToJobObject(self.0, process) } == 0 {
            return Err(std::io::Error::last_os_error()).context("assign process to bounded job");
        }
        Ok(())
    }
    pub fn terminate(&self) {
        unsafe {
            TerminateJobObject(self.0, 1);
        }
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
