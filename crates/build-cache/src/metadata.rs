//! Checksum-bound registry/archive metadata; parse declarations without executing them.
use crate::{MAX_FILES, files::read_bounded};
use anyhow::{Context, Result, ensure};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    time::Duration,
};

pub(super) fn json_hashes(value: &serde_json::Value, hashes: &mut BTreeSet<String>) -> Result<()> {
    match value {
        serde_json::Value::Object(object) => {
            for (key, value) in object {
                if key == "sha256" {
                    if let Some(digest) = value.as_str().filter(|v| sha256(v)) {
                        hashes.insert(digest.to_owned());
                    }
                } else if key == "integrity" {
                    if let Some(integrity) = value.as_str() {
                        for item in integrity.split_whitespace() {
                            if let Some(encoded) = item.strip_prefix("sha256-") {
                                hashes.insert(decode_sha256(encoded)?);
                            }
                        }
                    }
                } else if key == "patches"
                    && let Some(patches) = value.as_object()
                {
                    for digest in patches.values().filter_map(serde_json::Value::as_str) {
                        if sha256(digest) {
                            hashes.insert(digest.to_owned());
                        } else if let Some(encoded) = digest.strip_prefix("sha256-") {
                            hashes.insert(decode_sha256(encoded)?);
                        }
                    }
                }
                json_hashes(value, hashes)?;
            }
        }
        serde_json::Value::Array(array) => {
            for value in array {
                json_hashes(value, hashes)?;
            }
        }
        _ => {}
    }
    ensure!(hashes.len() <= MAX_FILES, "too many dependency hashes");
    Ok(())
}

pub(super) fn decode_sha256(encoded: &str) -> Result<String> {
    ensure!(
        encoded.len() == 44 && encoded.ends_with('='),
        "invalid SHA256 integrity length"
    );
    let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut bits = 0u32;
    let mut count = 0;
    let mut bytes = Vec::new();
    for byte in encoded.bytes().take(43) {
        let value = alphabet
            .iter()
            .position(|b| *b == byte)
            .context("invalid SHA256 integrity encoding")? as u32;
        bits = (bits << 6) | value;
        count += 6;
        if count >= 8 {
            count -= 8;
            bytes.push((bits >> count) as u8);
        }
    }
    ensure!(
        bytes.len() == 32 && bits & 3 == 0,
        "invalid SHA256 integrity padding"
    );
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

pub(super) fn verified_archive(
    cas: &Path,
    metadata: &serde_json::Value,
) -> Result<Option<PathBuf>> {
    let Some(integrity) = metadata
        .get("integrity")
        .and_then(serde_json::Value::as_str)
        .and_then(|value| value.strip_prefix("sha256-"))
    else {
        return Ok(None);
    };
    let digest = decode_sha256(integrity)?;
    let archive = cas.join(&digest).join("file");
    if !archive.exists() {
        return Ok(None);
    }
    let bytes = read_bounded(&archive, 128 * 1024 * 1024)?;
    ensure!(
        storage::digest(&bytes) == digest,
        "dependency metadata archive checksum mismatch"
    );
    Ok(Some(archive))
}

pub(super) fn read_tar_member(worktree: &Path, archive: &Path, member: &str) -> Result<String> {
    let output = process::capture(
        "tar",
        &["-xOf".into(), archive.display().to_string(), member.into()],
        worktree,
        Duration::from_secs(10),
    )?;
    ensure!(
        output.status.success(),
        "cannot read verified dependency metadata"
    );
    Ok(String::from_utf8(output.stdout)?)
}

pub(super) fn sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub(super) fn safe_component(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.+".contains(&b))
}

#[cfg(test)]
#[path = "metadata_tests.rs"]
mod tests;
