//! Directory adapter: descriptor-relative paths, optimistic revisions and durable publication receipts.
use anyhow::{Context, Result, ensure};
use collaboration_model::{KnowledgeWrite, KnowledgeWriteResult, Outcome};
use contracts::{KnowledgeError, KnowledgeErrorCode, KnowledgeWriteState};
mod scan;
use knowledge_model::{MAX_DOCUMENT_BYTES, Target, baseline, result};
use scan::{Budget, snapshot};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::Read,
    os::{fd::AsRawFd, unix::fs::OpenOptionsExt},
    path::{Component, Path, PathBuf},
};

fn open_directory(path: &Path) -> Result<File> {
    Ok(OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
        .open(path)?)
}
fn anchored(file: &File) -> PathBuf {
    PathBuf::from(format!("/proc/self/fd/{}", file.as_raw_fd()))
}
fn child(root: &File, path: &Path, create: bool) -> Result<File> {
    let mut current = root.try_clone()?;
    for c in path.components() {
        let Component::Normal(name) = c else {
            anyhow::bail!("knowledge_path_escape")
        };
        let next = anchored(&current).join(name);
        if create && !next.exists() {
            match fs::create_dir(&next) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e.into()),
            }
        }
        current = open_directory(&next)?;
    }
    Ok(current)
}
fn root(target: &Target) -> Result<File> {
    target.validate()?;
    let base = child(
        &open_directory(Path::new("/"))?,
        target.path.strip_prefix("/")?,
        false,
    )?;
    if target.content_root == Path::new(".") {
        Ok(base)
    } else {
        child(&base, &target.content_root, false)
    }
}
fn read(root: &File, path: &Path) -> Result<Option<Vec<u8>>> {
    knowledge_model::relative(path)?;
    let parent = match child(root, path.parent().unwrap_or(Path::new("")), false) {
        Ok(v) => v,
        Err(e)
            if e.downcast_ref::<std::io::Error>()
                .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) =>
        {
            return Ok(None);
        }
        Err(e) => return Err(e),
    };
    let p = anchored(&parent).join(path.file_name().context("knowledge_invalid_path")?);
    let mut f = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(p)
    {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let metadata = f.metadata()?;
    ensure!(
        metadata.is_file(),
        KnowledgeError::new(KnowledgeErrorCode::SpecialFileRejected)
            .parameter("path", path.display())
    );
    ensure!(
        metadata.len() <= MAX_DOCUMENT_BYTES as u64,
        KnowledgeError::new(KnowledgeErrorCode::DocumentLimit)
            .parameter("path", path.display())
            .parameter("actual", metadata.len())
            .parameter("limit", MAX_DOCUMENT_BYTES)
    );
    let mut bytes = Vec::new();
    Read::by_ref(&mut f)
        .take((MAX_DOCUMENT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= MAX_DOCUMENT_BYTES,
        KnowledgeError::new(KnowledgeErrorCode::DocumentLimit)
            .parameter("path", path.display())
            .parameter("actual", bytes.len())
            .parameter("limit", MAX_DOCUMENT_BYTES)
    );
    Ok(Some(bytes))
}
pub fn inspect(
    target: &Target,
    path: Option<&Path>,
    expected: Option<&str>,
) -> Result<(String, Option<String>)> {
    let mut budget = Budget::new();
    let root = root(target)?;
    let revision = snapshot(&root, &mut budget)?;
    if let Some(expected) = expected {
        ensure!(
            expected == revision,
            KnowledgeError::new(KnowledgeErrorCode::RevisionConflict)
        );
    }
    if let Some(path) = path {
        scan::allowed(&root, path, &mut budget)?;
    }
    let bytes = path.map(|p| read(&root, p)).transpose()?.flatten();
    if let Some(bytes) = &bytes {
        budget.consume(path.unwrap(), bytes.len())?;
    }
    let content = bytes.map(String::from_utf8).transpose()?;
    ensure!(
        snapshot(&root, &mut budget)? == revision,
        KnowledgeError::new(KnowledgeErrorCode::ContentChanged)
    );
    Ok((revision, content))
}
#[derive(Clone, Serialize, Deserialize)]
struct Journal {
    request: KnowledgeWrite,
    path: PathBuf,
    before: Option<String>,
    after: Outcome,
}
#[derive(Serialize, Deserialize)]
struct Receipt {
    request: KnowledgeWrite,
    result: KnowledgeWriteResult,
    content_digest: String,
}
fn private(root: &File) -> Result<File> {
    child(root, Path::new(".outcomehub"), true)
}
fn receipt(root: &File, write: &KnowledgeWrite) -> Result<Option<KnowledgeWriteResult>> {
    let private = match child(root, Path::new(".outcomehub"), false) {
        Ok(v) => v,
        Err(e)
            if e.downcast_ref::<std::io::Error>()
                .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) =>
        {
            return Ok(None);
        }
        Err(e) => return Err(e),
    };
    let p = anchored(&private).join(format!("{}.json", write.input.id));
    if !p.exists() {
        return Ok(None);
    }
    let entry: Receipt = storage::read(&p)?;
    ensure!(
        entry.request.request_digest == write.request_digest && entry.request.input == write.input,
        "knowledge_request_conflict"
    );
    let content = read(root, Path::new(&entry.result.reference))?
        .context("knowledge_receipt_content_missing")?;
    ensure!(
        storage::digest(&content) == entry.content_digest,
        "knowledge_receipt_content_changed"
    );
    Ok(Some(entry.result))
}
pub fn lookup(
    target: &Target,
    state: &Path,
    write: &KnowledgeWrite,
) -> Result<Option<KnowledgeWriteResult>> {
    let mut budget = Budget::new();
    let root = root(target)?;
    if let Some(receipt) = receipt(&root, write)? {
        return Ok(Some(receipt));
    }
    let journal_path = state.join(format!("{}.journal.json", write.input.id));
    if !journal_path.exists() {
        return Ok(None);
    }
    let journal: Journal = storage::read(&journal_path)?;
    ensure!(
        journal.request.request_digest == write.request_digest
            && journal.request.input == write.input,
        "knowledge_request_conflict"
    );
    let after = serde_json::to_vec_pretty(&journal.after)?;
    scan::allowed(&root, &journal.path, &mut budget)?;
    let current = read(&root, &journal.path)?;
    if current.as_deref() == Some(after.as_slice()) {
        // Recover the durable local receipt without changing external files or repeating a write.
        return Ok(Some(result(
            write,
            KnowledgeWriteState::Applied,
            journal
                .path
                .to_str()
                .context("knowledge_invalid_path")?
                .into(),
            snapshot(&root, &mut budget)?,
            "Publication recovered from the durable journal and matching external content",
        )));
    }
    Ok(None)
}
pub fn publish(
    target: &Target,
    state: &Path,
    write: &KnowledgeWrite,
    outcome: &Outcome,
) -> Result<KnowledgeWriteResult> {
    let mut budget = Budget::new();
    let root = root(target)?;
    let private = private(&root)?;
    // Lock files are opened without following links, including the final component.
    let lock_path = anchored(&private).join("writer.lock");
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&lock_path)?;
    ensure!(
        unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0,
        "knowledge_target_busy"
    );
    if let Some(value) = receipt(&root, write)? {
        return Ok(value);
    }
    let path = PathBuf::from(format!("accepted-outcomes/{}.json", outcome.input.id));
    scan::allowed(&root, &path, &mut budget)?;
    let journal_path = state.join(format!("{}.journal.json", write.input.id));
    let current = read(&root, &path)?;
    let after = serde_json::to_vec_pretty(outcome)?;
    let before = current.as_ref().map(|v| storage::digest(v));
    let journal = if journal_path.exists() {
        let value: Journal = storage::read(&journal_path)?;
        ensure!(
            value.request.request_digest == write.request_digest
                && value.request.input == write.input
                && value.after.digest == outcome.digest,
            "knowledge_request_conflict"
        );
        ensure!(
            before == value.before || current.as_deref() == Some(after.as_slice()),
            "knowledge_recovery_conflict"
        );
        value
    } else {
        ensure!(
            snapshot(&root, &mut budget)? == baseline(&write.input.expected_revision),
            KnowledgeError::new(KnowledgeErrorCode::RevisionConflict)
        );
        let value = Journal {
            request: write.clone(),
            path: path.clone(),
            before,
            after: outcome.clone(),
        };
        storage::write(&journal_path, &value)?;
        value
    };
    let parent = child(&root, path.parent().unwrap(), true)?;
    let destination = anchored(&parent).join(path.file_name().unwrap());
    let now = read(&root, &path)?.map(|v| storage::digest(&v));
    ensure!(
        now == journal.before || now == Some(storage::digest(&after)),
        "knowledge_revision_conflict"
    );
    storage::write(&destination, outcome)?;
    let revision = snapshot(&root, &mut budget)?;
    let result = result(
        write,
        KnowledgeWriteState::Applied,
        path.to_str().unwrap().into(),
        revision,
        "Accepted outcome published to the configured directory",
    );
    storage::write(
        &anchored(&private).join(format!("{}.json", write.input.id)),
        &Receipt {
            request: write.clone(),
            result: result.clone(),
            content_digest: storage::digest(&after),
        },
    )?;
    fs::remove_file(journal_path)?;
    Ok(result)
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
