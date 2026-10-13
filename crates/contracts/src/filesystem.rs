//! Metadata for opt-in Native-parent filesystem observation, never file bodies.
use crate::{
    NativeCaptureClass, NativeFilesystemCategory, NativeFilesystemCompleteness,
    NativeFilesystemOperation, NativeFilesystemResult, NativeFilesystemScope,
};
use serde::{Deserialize, Serialize};

pub const NATIVE_FILESYSTEM_EVENTS: u64 = 8192;
pub const NATIVE_FILESYSTEM_NORMAL_EVENTS: u64 = 6144;
pub const NATIVE_FILESYSTEM_CLEANUP_EVENTS: u64 = 2048;
pub const NATIVE_FILESYSTEM_PATHS: usize = 2048;
pub const NATIVE_FILESYSTEM_SITES: usize = 8192;
pub const NATIVE_FILESYSTEM_ATTEMPTS: usize = 32;
pub const NATIVE_FILESYSTEM_PATHS_PER_OPERATION: usize = 2;
pub const NATIVE_FILESYSTEM_REQUEST_BYTES: u64 = 512 * 1024 * 1024;
pub const NATIVE_FILESYSTEM_RECORD_BYTES: usize = 768;

impl NativeFilesystemOperation {
    /// Exact expected OS failures that do not, alone, revoke observation completeness.
    /// Callers must still validate the actual return, metadata, clock and publication.
    pub fn permits_failure(self, errno: Option<i32>) -> bool {
        use NativeFilesystemOperation::*;
        matches!(
            (self, errno),
            (OpenWrite | CreateDirectory | HardLink, Some(libc::EEXIST))
                | (Unlink | RemoveDirectory, Some(libc::ENOENT))
                | (Write, Some(libc::EINTR))
        )
    }
}

fn required_option<'de, D, T>(d: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "[u64; 6]", into = "[u64; 6]")]
pub struct NativeFilesystemIdentity {
    pub device: u64,
    pub inode: u64,
    pub mode: u32,
    pub uid: u32,
    pub links: u64,
    pub bytes: u64,
}

impl From<NativeFilesystemIdentity> for [u64; 6] {
    fn from(v: NativeFilesystemIdentity) -> Self {
        [
            v.device,
            v.inode,
            u64::from(v.mode),
            u64::from(v.uid),
            v.links,
            v.bytes,
        ]
    }
}
impl TryFrom<[u64; 6]> for NativeFilesystemIdentity {
    type Error = &'static str;
    fn try_from(v: [u64; 6]) -> Result<Self, Self::Error> {
        Ok(Self {
            device: v[0],
            inode: v[1],
            mode: v[2].try_into().map_err(|_| "filesystem mode overflow")?,
            uid: v[3].try_into().map_err(|_| "filesystem uid overflow")?,
            links: v[4],
            bytes: v[5],
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeFilesystemAttempt {
    pub task_id: String,
    pub run_id: String,
    pub attempt: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeFilesystemBinding {
    pub schema_version: u32,
    pub session_sha256: String,
    pub source_sha256: String,
    pub config_sha256: String,
    pub allowlist_sha256: String,
    pub pid: u32,
    pub process_start_ticks: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeFilesystemPath {
    pub root: u16,
    pub relative: String,
    pub category: NativeFilesystemCategory,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeFilesystemRequest {
    pub operation: NativeFilesystemOperation,
    pub class: NativeCaptureClass,
    pub scope: NativeFilesystemScope,
    #[serde(deserialize_with = "required_option")]
    pub attempt: Option<NativeFilesystemAttempt>,
    pub callsite: String,
    pub paths: Vec<NativeFilesystemPath>,
    /// Reservation upper bound, not evidence that these bytes were written.
    pub requested_bytes: u64,
    #[serde(deserialize_with = "required_option")]
    pub control_span: Option<u16>,
    #[serde(deserialize_with = "required_option")]
    pub flags: Option<i32>,
    #[serde(deserialize_with = "required_option")]
    pub mode: Option<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeFilesystemActual {
    #[serde(rename = "r")]
    pub result: NativeFilesystemResult,
    #[serde(rename = "e")]
    #[serde(deserialize_with = "required_option")]
    pub errno: Option<i32>,
    /// Only bytes returned by successful write syscalls. None means unknown.
    #[serde(rename = "w")]
    #[serde(deserialize_with = "required_option")]
    pub written_bytes: Option<u64>,
    #[serde(rename = "v")]
    #[serde(deserialize_with = "required_option")]
    pub return_value: Option<i64>,
    #[serde(rename = "new")]
    #[serde(deserialize_with = "required_option")]
    pub created: Option<bool>,
    #[serde(rename = "b")]
    pub before: Vec<Option<NativeFilesystemIdentity>>,
    #[serde(rename = "a")]
    pub after: Vec<Option<NativeFilesystemIdentity>>,
    #[serde(rename = "s")]
    pub started_ns: u64,
    #[serde(rename = "t")]
    #[serde(deserialize_with = "required_option")]
    pub ended_ns: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeFilesystemReport {
    pub schema_version: u32,
    pub binding: NativeFilesystemBinding,
    pub completeness: NativeFilesystemCompleteness,
    pub reserved: u64,
    pub finalized: u64,
    pub outstanding: u32,
    #[serde(deserialize_with = "required_option")]
    pub fault: Option<String>,
}

/// Compact metadata rows. Indices join the same envelope tables, never worker state.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeFilesystemRecord {
    #[serde(rename = "i")]
    pub ordinal: u64,
    #[serde(rename = "z", deserialize_with = "required_option")]
    pub control_span: Option<u16>,
    #[serde(rename = "o")]
    pub operation: NativeFilesystemOperation,
    #[serde(rename = "c")]
    pub class: NativeCaptureClass,
    #[serde(rename = "k")]
    pub scope: NativeFilesystemScope,
    #[serde(rename = "n")]
    #[serde(deserialize_with = "required_option")]
    pub attempt: Option<u16>,
    #[serde(rename = "f")]
    pub site: u16,
    #[serde(rename = "p")]
    pub paths: Vec<u16>,
    #[serde(rename = "q")]
    pub requested_bytes: u64,
    #[serde(rename = "g")]
    #[serde(deserialize_with = "required_option")]
    pub flags: Option<i32>,
    #[serde(rename = "m")]
    #[serde(deserialize_with = "required_option")]
    pub mode: Option<u32>,
    #[serde(rename = "x")]
    pub actual: NativeFilesystemActual,
}
impl NativeFilesystemRecord {
    /// Structural bounds only. Consumers must also join tables, clocks and actual OS facts.
    pub fn validate_bounds(&self) -> Result<(), &'static str> {
        if self.ordinal >= NATIVE_FILESYSTEM_EVENTS
            || self
                .control_span
                .is_some_and(|v| u64::from(v) >= NATIVE_FILESYSTEM_EVENTS)
            || self
                .attempt
                .is_some_and(|v| usize::from(v) >= NATIVE_FILESYSTEM_ATTEMPTS)
            || usize::from(self.site) >= NATIVE_FILESYSTEM_SITES
            || self.paths.is_empty()
            || self.paths.len() > NATIVE_FILESYSTEM_PATHS_PER_OPERATION
            || self
                .paths
                .iter()
                .any(|v| usize::from(*v) >= NATIVE_FILESYSTEM_PATHS)
            || self.requested_bytes > NATIVE_FILESYSTEM_REQUEST_BYTES
            || self.actual.before.len() > NATIVE_FILESYSTEM_PATHS_PER_OPERATION
            || self.actual.after.len() > NATIVE_FILESYSTEM_PATHS_PER_OPERATION
        {
            return Err("filesystem record structural bound");
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeFilesystemEnvelope {
    pub session: NativeFilesystemReport,
    pub sites: Vec<String>,
    pub attempts: Vec<NativeFilesystemAttempt>,
    pub paths: Vec<NativeFilesystemPath>,
    pub records: Vec<NativeFilesystemRecord>,
}
