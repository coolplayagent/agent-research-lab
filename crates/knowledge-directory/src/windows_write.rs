//! Rename the owned file within its pinned parent, without reopening that directory for writes.
use super::{Directory, anchored};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use std::{
    ffi::c_void,
    fs::{self, OpenOptions},
    io::Write,
    os::windows::{ffi::OsStrExt, fs::OpenOptionsExt, io::AsRawHandle},
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
#[repr(C)]
struct IoStatus {
    status: isize,
    information: usize,
}
#[repr(C)]
struct Rename {
    replace: u8,
    root: *mut c_void,
    length: u32,
    name: [u16; 1],
}
#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtSetInformationFile(
        file: *mut c_void,
        status: *mut IoStatus,
        information: *const c_void,
        length: u32,
        class: u32,
    ) -> i32;
    fn RtlNtStatusToDosError(status: i32) -> u32;
}
fn rename(file: &fs::File, name: &Path) -> Result<()> {
    let name: Vec<u16> = name.as_os_str().encode_wide().collect();
    let bytes = size_of::<Rename>() + name.len() * 2;
    let mut buffer = vec![0usize; bytes.div_ceil(size_of::<usize>())];
    let mut status = IoStatus {
        status: 0,
        information: 0,
    };
    // FILE_RENAME_INFORMATION with a simple name and NULL root performs a rename
    // inside the same directory; every ancestor remains pinned throughout.
    // https://learn.microsoft.com/windows-hardware/drivers/ddi/ntifs/ns-ntifs-_file_rename_information
    let result = unsafe {
        let header = buffer.as_mut_ptr().cast::<Rename>();
        header.write(Rename {
            replace: 1,
            root: std::ptr::null_mut(),
            length: (name.len() * 2) as u32,
            name: [0],
        });
        std::ptr::copy_nonoverlapping(
            name.as_ptr(),
            buffer
                .as_mut_ptr()
                .cast::<u8>()
                .add(std::mem::offset_of!(Rename, name))
                .cast::<u16>(),
            name.len(),
        );
        NtSetInformationFile(
            file.as_raw_handle(),
            &mut status,
            buffer.as_ptr().cast(),
            bytes as u32,
            10,
        )
    };
    if result < 0 {
        return Err(std::io::Error::from_raw_os_error(
            unsafe { RtlNtStatusToDosError(result) } as i32,
        ))
        .context("rename owned knowledge file");
    }
    Ok(())
}
pub(crate) fn write<T: Serialize>(parent: &Directory, name: &Path, value: &T) -> Result<()> {
    knowledge_model::relative(name)?;
    ensure!(name.components().count() == 1, "knowledge_invalid_path");
    let bytes = serde_json::to_vec_pretty(value)?;
    ensure!(bytes.len() <= 16 * 1024 * 1024, "knowledge_document_limit");
    let temporary = anchored(parent).join(format!(
        ".pending-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| -> Result<()> {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .access_mode(0x4000_0000 | 0x0001_0000)
            .share_mode(1)
            .custom_flags(super::OPEN_REPARSE)
            .open(&temporary)?;
        storage::private_file(&temporary)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        rename(&file, name)?;
        file.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

#[cfg(test)]
#[path = "windows_write_tests.rs"]
mod tests;
