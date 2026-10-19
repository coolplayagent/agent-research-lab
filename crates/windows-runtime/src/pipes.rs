use crate::Process;
use anyhow::{Context, Result};
use std::{fs::File, os::windows::io::FromRawHandle, path::Path, ptr};
use windows_sys::Win32::System::Pipes::CreatePipe;
fn pipe() -> Result<(File, File)> {
    let mut reader = ptr::null_mut();
    let mut writer = ptr::null_mut();
    if unsafe { CreatePipe(&mut reader, &mut writer, ptr::null(), 65536) } == 0 {
        return Err(std::io::Error::last_os_error()).context("create bounded agent pipe");
    }
    Ok(unsafe { (File::from_raw_handle(reader), File::from_raw_handle(writer)) })
}
/// Pipe endpoints are attached to a suspended process before it joins its job.
pub fn spawn_piped(
    command: &[String],
    cwd: &Path,
    environment: &[(String, String)],
) -> Result<(Process, File, File, File)> {
    let (input_reader, input_writer) = pipe()?;
    let (output_reader, output_writer) = pipe()?;
    let (error_reader, error_writer) = pipe()?;
    let process = Process::spawn(
        None,
        command,
        cwd,
        environment,
        false,
        &input_reader,
        &output_writer,
        &error_writer,
    )?;
    Ok((process, input_writer, output_reader, error_reader))
}
