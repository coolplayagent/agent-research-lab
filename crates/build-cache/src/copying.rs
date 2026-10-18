//! Shared private-copy accounting, deadlines, verification and atomic replacement.
use crate::{
    MAX_BYTES, MAX_FILE_BYTES, MAX_FILES,
    files::{checked_directory, regular_file},
    report::{PreparationDeferred, SeedReport},
};
use anyhow::{Context, Result, ensure};
use native_effects::fs::{self, OpenOptions};
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
static SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(super) struct Seeder {
    started: Instant,
    pub(super) report: SeedReport,
}

impl Seeder {
    pub(super) fn new(started: Instant, report: SeedReport) -> Self {
        Self { started, report }
    }
    pub(super) fn finish(mut self) -> SeedReport {
        self.report.elapsed_millis = self.started.elapsed().as_millis() as u64;
        self.report
    }
    pub(super) fn deadline(&self) -> Result<()> {
        if process::deadline_exhausted() || self.started.elapsed() >= Duration::from_secs(55) {
            return Err(PreparationDeferred.into());
        }
        Ok(())
    }

    pub(super) fn copy(
        &mut self,
        source: &Path,
        destination: &Path,
        expected: Option<&str>,
        executable: bool,
    ) -> Result<()> {
        self.deadline()?;
        let mut input = regular_file(source)?;
        let size = input.metadata()?.len();
        ensure!(
            size <= MAX_FILE_BYTES
                && self.report.retained_files < MAX_FILES
                && self.report.retained_bytes + size <= MAX_BYTES,
            "build cache copy budget exceeded"
        );
        self.report.retained_files += 1;
        self.report.retained_bytes += size;
        checked_directory(
            destination
                .parent()
                .context("cache destination lacks parent")?,
            true,
        )?;
        if destination.exists() {
            let expected = if let Some(expected) = expected {
                expected.to_owned()
            } else {
                self.hash(source)?
            };
            if self.hash(destination)? == expected {
                self.report.reused_files += 1;
                return Ok(());
            }
        }
        let temporary = destination.with_extension(format!(
            "seed-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let result = (|| -> Result<()> {
            fs::check_write_bound(&temporary, size)?;
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW)
                .open(&temporary)?;
            let mut hash = Sha256::new();
            let mut buffer = [0u8; 64 * 1024];
            let mut copied = 0u64;
            loop {
                self.deadline()?;
                let count = input.read(&mut buffer)?;
                if count == 0 {
                    break;
                }
                copied += count as u64;
                ensure!(copied <= size, "cache source grew during copy");
                fs::write_all(&mut output, &temporary, &buffer[..count])?;
                hash.update(&buffer[..count]);
            }
            ensure!(copied == size, "cache source changed during copy");
            let digest = format!("{:x}", hash.finalize());
            ensure!(
                expected.is_none_or(|expected| digest == expected),
                "cache checksum mismatch: {}",
                source.display()
            );
            if executable {
                fs::set_file_permissions(&output, &temporary, fs::Permissions::from_mode(0o700))?;
            }
            drop(output);
            fs::rename(&temporary, destination)?;
            Ok(())
        })();
        if result.is_err() {
            let _cleanup = native_effects::cleanup_scope();
            let _ = fs::remove_file(&temporary);
        }
        result?;
        self.report.copied_files += 1;
        self.report.copied_bytes += size;
        Ok(())
    }

    fn hash(&self, path: &Path) -> Result<String> {
        let mut file = regular_file(path)?;
        let size = file.metadata()?.len();
        ensure!(size <= MAX_FILE_BYTES, "cache file exceeds byte limit");
        let mut hash = Sha256::new();
        let mut buffer = [0u8; 64 * 1024];
        let mut count = 0;
        loop {
            self.deadline()?;
            let n = file.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            count += n as u64;
            ensure!(count <= size, "cache source grew during verification");
            hash.update(&buffer[..n]);
        }
        ensure!(count == size, "cache source changed during verification");
        Ok(format!("{:x}", hash.finalize()))
    }
}

#[cfg(test)]
#[path = "copying_tests.rs"]
mod tests;
