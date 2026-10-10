//! Explicit delivery adapters. No network or installation happens merely by loading a request.
use anyhow::{Context, Result, bail, ensure};
use freshness::{RemoteClient, RemoteSnapshot, SkillsManifest};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VerificationArtifact {
    pub path: PathBuf,
    pub sha256: String,
}

impl VerificationArtifact {
    pub fn read(&self) -> Result<Value> {
        validate_digest(&self.sha256)?;
        ensure!(
            fs::symlink_metadata(&self.path)?.file_type().is_file(),
            "verification evidence must be a regular file"
        );
        let bytes = fs::read(&self.path)?;
        ensure!(
            format!("{:x}", Sha256::digest(&bytes)) == self.sha256,
            "verification artifact changed"
        );
        serde_json::from_slice(&bytes).context("verification artifact must be JSON")
    }

    pub fn validate_skills(&self) -> Result<()> {
        let manifest: SkillsManifest = serde_json::from_value(self.read()?)?;
        ensure!(
            manifest.schema_version == 1 && !manifest.skills.is_empty(),
            "skills manifest must be a nonempty version-one host artifact"
        );
        Ok(())
    }

    pub fn verify_skills(&self, remote: &RemoteClient) -> Result<()> {
        self.validate_skills()?;
        remote.verify_skills(&self.path)?;
        // Do not permit a replacement between the bound hash check and the
        // live verification to silently switch which skills were evaluated.
        self.validate_skills()?;
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReleaseEvidence {
    pub source_commit: String,
    /// Optional only for reading historical receipts. Every new delivery gate
    /// requires a bound, currently fresh remote-default baseline.
    #[serde(default)]
    pub upstream: Option<RemoteSnapshot>,
    #[serde(default)]
    pub skills_manifest: Option<VerificationArtifact>,
    pub prompt_sha256: String,
    pub policy_sha256: String,
    pub superpod_commit: String,
    pub implementer_id: String,
    pub evaluator_id: String,
    pub evaluation_passed: bool,
    pub qualitygate_passed: bool,
    pub qualitygate_report: VerificationArtifact,
    /// Host-produced snapshot binding; candidate agents must not own this receipt.
    pub snapshot_receipt: VerificationArtifact,
    /// Host-collected result of a distinct evaluator invocation, with exact input bindings.
    pub evaluation_receipt: VerificationArtifact,
}

impl ReleaseEvidence {
    pub fn validate(&self) -> Result<()> {
        let upstream = self.upstream.as_ref().context(
            "old release evidence lacks an upstream baseline; re-evaluation is required",
        )?;
        upstream.validate()?;
        let skills = self.skills_manifest.as_ref().context(
            "old release evidence lacks a hashed skills manifest; re-evaluation is required",
        )?;
        skills.validate_skills()?;
        validate_commit(&self.source_commit)?;
        validate_commit(&self.superpod_commit)?;
        validate_digest(&self.prompt_sha256)?;
        validate_digest(&self.policy_sha256)?;
        ensure!(
            self.evaluation_passed && self.qualitygate_passed,
            "independent evaluation and Qualitygate must pass"
        );
        ensure!(
            !self.implementer_id.is_empty()
                && !self.evaluator_id.is_empty()
                && self.implementer_id != self.evaluator_id,
            "evaluation must identify a distinct evaluator"
        );
        let qualitygate = self.qualitygate_report.read()?;
        ensure!(
            qualitygate["profile"] == "full"
                && ["repository", "delivery", "task"]
                    .contains(&qualitygate["scope"].as_str().unwrap_or("")),
            "Qualitygate report must cover a full delivery, task or repository scope"
        );
        ensure!(
            qualitygate["gate"]["complete"] == true
                && qualitygate["gate"]["decision"] == contracts::VerificationVerdict::Pass.as_str(),
            "Qualitygate report is incomplete or failed"
        );
        ensure!(
            qualitygate["plan"]["pending_delivery_checks"]
                .as_array()
                .is_some_and(Vec::is_empty),
            "Qualitygate has pending or unknown checks"
        );
        let snapshot = self.snapshot_receipt.read()?;
        let evaluation = self.evaluation_receipt.read()?;
        for receipt in [&snapshot, &evaluation] {
            ensure!(
                receipt["skills_manifest_sha256"].as_str() == Some(&skills.sha256),
                "verification receipt has stale skills manifest binding"
            );
            let bound: RemoteSnapshot = serde_json::from_value(receipt["upstream"].clone())
                .context("verification receipt lacks a valid upstream baseline")?;
            bound.validate()?;
            ensure!(
                bound.same_binding(upstream),
                "verification receipt has stale upstream baseline"
            );
            for (key, expected) in [
                ("source_commit", &self.source_commit),
                ("prompt_sha256", &self.prompt_sha256),
                ("policy_sha256", &self.policy_sha256),
                ("superpod_commit", &self.superpod_commit),
            ] {
                ensure!(
                    receipt[key].as_str() == Some(expected),
                    "verification receipt has stale {key}"
                );
            }
        }
        ensure!(
            snapshot["qualitygate_report_sha256"].as_str() == Some(&self.qualitygate_report.sha256),
            "snapshot receipt refers to another Qualitygate report"
        );
        ensure!(
            snapshot["content_digest"].is_string()
                && snapshot["content_digest"] == qualitygate["snapshot"]["content_digest"]
                && snapshot["verification_digest"]
                    == qualitygate["snapshot"]["verification_digest"],
            "Qualitygate snapshot differs from host-collected source snapshot"
        );
        ensure!(
            evaluation["role"] == "independent_evaluator"
                && evaluation["status"] == contracts::VerificationVerdict::Pass.as_str()
                && evaluation["complete"] == true,
            "independent evaluation receipt is incomplete or failed"
        );
        ensure!(
            evaluation["evaluator_id"].as_str() == Some(&self.evaluator_id)
                && evaluation["implementer_id"].as_str() == Some(&self.implementer_id),
            "evaluator invocation identity mismatch"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InstallRequest {
    pub skill_source: PathBuf,
    pub skills_root: PathBuf,
    /// Outside skills_root, on the same filesystem so directory renames are atomic.
    pub backup_root: PathBuf,
    pub skill_directory_name: String,
    pub binary_relative_path: PathBuf,
    pub binary_sha256: String,
    pub version: String,
    pub evidence: ReleaseEvidence,
    pub smoke_arguments: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InstallReceipt {
    pub operation_id: String,
    pub target: PathBuf,
    pub backup: Option<PathBuf>,
    pub previous_sha256: Option<String>,
    pub package_sha256: String,
    pub binary_sha256: String,
    pub binary_relative_path: PathBuf,
    pub version: String,
    pub evidence: ReleaseEvidence,
    /// staged, previous_backed_up, activated, verified, rolled_back.
    pub phase: String,
    pub receipt_path: PathBuf,
}

pub fn file_sha256(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

pub fn validate_digest(value: &str) -> Result<()> {
    ensure!(
        value.len() == 64 && value.bytes().all(|c| c.is_ascii_hexdigit()),
        "expected a full SHA-256 digest"
    );
    Ok(())
}

pub fn validate_commit(value: &str) -> Result<()> {
    ensure!(
        (value.len() == 40 || value.len() == 64) && value.bytes().all(|c| c.is_ascii_hexdigit()),
        "expected a full immutable Git commit"
    );
    Ok(())
}

pub fn validate_identifier(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.len() <= 100
            && !value.starts_with('.')
            && value
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_'),
        "invalid identifier"
    );
    Ok(())
}

pub fn relative_path(path: &Path) -> Result<()> {
    ensure!(
        !path.as_os_str().is_empty()
            && path.components().all(|p| matches!(p, Component::Normal(_))),
        "path must be confined and relative"
    );
    Ok(())
}

fn files(root: &Path, current: &Path, result: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        ensure!(
            !kind.is_symlink(),
            "symlinks are not accepted in install packages"
        );
        if kind.is_dir() {
            files(root, &entry.path(), result)?;
        } else {
            ensure!(kind.is_file(), "only regular package files are supported");
            result.push(entry.path().strip_prefix(root)?.to_owned());
        }
    }
    Ok(())
}

fn tree_sha256(root: &Path) -> Result<String> {
    let mut paths = Vec::new();
    files(root, root, &mut paths)?;
    paths.sort();
    let mut hash = Sha256::new();
    for path in paths {
        hash.update(path.to_string_lossy().as_bytes());
        hash.update([0]);
        hash.update(file_sha256(&root.join(&path))?.as_bytes());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            hash.update(
                fs::metadata(root.join(path))?
                    .permissions()
                    .mode()
                    .to_le_bytes(),
            );
        }
    }
    Ok(format!("{:x}", hash.finalize()))
}

/// Content-and-mode digest used by the host's build/snapshot receipt.
pub fn package_sha256(root: &Path) -> Result<String> {
    tree_sha256(root)
}

fn copy_package(source: &Path, target: &Path) -> Result<()> {
    fs::create_dir(target)?;
    let mut paths = Vec::new();
    files(source, source, &mut paths)?;
    for path in paths {
        if let Some(parent) = target.join(&path).parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(source.join(&path), target.join(&path))?;
    }
    Ok(())
}

pub fn unique_id() -> String {
    format!(
        "{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    )
}

fn write_receipt(receipt: &InstallReceipt) -> Result<()> {
    let temporary = receipt.receipt_path.with_extension("tmp");
    let mut file = File::create(&temporary)?;
    file.write_all(&serde_json::to_vec_pretty(receipt)?)?;
    file.sync_all()?;
    fs::rename(temporary, &receipt.receipt_path)?;
    File::open(
        receipt
            .receipt_path
            .parent()
            .context("missing receipt parent")?,
    )?
    .sync_all()?;
    Ok(())
}

fn installation_lock(root: &Path, name: &str) -> Result<storage::LockGuard> {
    fs::create_dir_all(root)?;
    storage::lock(&root.join(format!("{name}.lock")))
        .context("another installation or rollback is active")
}

fn skill_name(path: &Path) -> Result<String> {
    let text =
        fs::read_to_string(path.join("SKILL.md")).context("package must contain SKILL.md")?;
    ensure!(
        text.starts_with("---\n") || text.starts_with("---\r\n"),
        "SKILL.md must have frontmatter"
    );
    let name = text
        .lines()
        .skip(1)
        .take_while(|line| *line != "---")
        .find_map(|line| line.strip_prefix("name:"))
        .context("SKILL.md must name the skill")?;
    let name = name.trim().trim_matches(['\'', '"']).to_owned();
    validate_identifier(&name)?;
    Ok(name)
}

/// Activate a packaged skill and its bundled CLI together, then smoke-test that exact binary.
/// Installation must be called at a task boundary; active tasks retain their pinned versions.
pub fn install_candidate(request: &InstallRequest) -> Result<InstallReceipt> {
    install_candidate_with_client(request, &RemoteClient::default())
}

fn install_candidate_with_client(
    request: &InstallRequest,
    remote: &RemoteClient,
) -> Result<InstallReceipt> {
    request.evidence.validate()?;
    let upstream = request
        .evidence
        .upstream
        .as_ref()
        .context("missing release upstream baseline")?;
    remote.verify_remote_binding(upstream)?;
    let skills_manifest = request
        .evidence
        .skills_manifest
        .as_ref()
        .context("missing release skills manifest")?;
    skills_manifest.verify_skills(remote)?;
    validate_identifier(&request.skill_directory_name)?;
    relative_path(&request.binary_relative_path)?;
    validate_digest(&request.binary_sha256)?;
    ensure!(
        !request.version.trim().is_empty() && !request.smoke_arguments.is_empty(),
        "version and smoke arguments are required"
    );
    let source = fs::canonicalize(&request.skill_source)?;
    ensure!(
        !fs::canonicalize(&skills_manifest.path)?.starts_with(&source),
        "skills manifest must be a host artifact outside the candidate package"
    );
    let name = skill_name(&source)?;
    let _lock = installation_lock(&request.backup_root, &request.skill_directory_name)?;
    fs::create_dir_all(&request.skills_root)?;
    let skills_root = fs::canonicalize(&request.skills_root)?;
    let backup_root = fs::canonicalize(&request.backup_root)?;
    ensure!(
        !backup_root.starts_with(&skills_root) && !skills_root.starts_with(&backup_root),
        "backups must be outside skill discovery"
    );
    ensure!(
        !source.starts_with(&skills_root),
        "candidate package must be staged outside installed skills"
    );
    let target = skills_root.join(&request.skill_directory_name);
    // A crash can occur between either rename and its journal update. Reconcile actual hashes.
    for entry in fs::read_dir(&backup_root)? {
        let path = entry?.path().join("receipt.json");
        if path.is_file() {
            let mut pending: InstallReceipt = serde_json::from_slice(&fs::read(&path)?)?;
            if pending.target == target
                && !["verified", "rolled_back"].contains(&pending.phase.as_str())
            {
                rollback_locked(&mut pending)
                    .context("recover unfinished installation before retrying")?;
            }
        }
    }
    if target.exists() {
        ensure!(
            !fs::symlink_metadata(&target)?.file_type().is_symlink(),
            "installed target cannot be a symlink"
        );
    }
    for entry in fs::read_dir(&skills_root)? {
        let path = entry?.path();
        if path != target && path.join("SKILL.md").is_file() {
            ensure!(
                skill_name(&path)? != name,
                "another installed directory already exposes this skill name: {}",
                path.display()
            );
        }
    }
    let source_hash = tree_sha256(&source)?;
    ensure!(
        file_sha256(&source.join(&request.binary_relative_path))? == request.binary_sha256,
        "binary digest mismatch"
    );
    let snapshot = request.evidence.snapshot_receipt.read()?;
    ensure!(
        snapshot["package_sha256"].as_str() == Some(&source_hash)
            && snapshot["binary_sha256"].as_str() == Some(&request.binary_sha256),
        "package is not the artifact verified by the host"
    );
    if target.exists() {
        let active_hash = tree_sha256(&target)?;
        for entry in fs::read_dir(&backup_root)? {
            let receipt_path = entry?.path().join("receipt.json");
            if receipt_path.is_file() {
                let receipt: InstallReceipt = serde_json::from_slice(&fs::read(receipt_path)?)?;
                if receipt.target == target
                    && receipt.phase == "verified"
                    && receipt.package_sha256 == active_hash
                    && receipt.version == request.version
                    && receipt.binary_sha256 == request.binary_sha256
                    && serde_json::to_value(&receipt.evidence)?
                        == serde_json::to_value(&request.evidence)?
                {
                    let provenance: Value = serde_json::from_slice(&fs::read(
                        target.join("agent-research-install.json"),
                    )?)?;
                    if provenance["source_package_sha256"].as_str() == Some(&source_hash) {
                        return Ok(receipt);
                    }
                }
            }
        }
    }
    let operation_id = unique_id();
    let operation = backup_root.join(format!("{}-{operation_id}", request.skill_directory_name));
    fs::create_dir(&operation)?;
    let staging = operation.join("candidate");
    copy_package(&source, &staging)?;
    ensure!(
        tree_sha256(&staging)? == source_hash,
        "package changed while staging"
    );
    // Metadata is part of the installed package hash; local development provenance is explicit.
    fs::write(
        staging.join("agent-research-install.json"),
        serde_json::to_vec_pretty(&json!({
            "version": request.version, "development_build": true, "source_package_sha256": source_hash,
            "binary_sha256": request.binary_sha256, "evidence": request.evidence,
        }))?,
    )?;
    let mut receipt = InstallReceipt {
        operation_id,
        target: target.clone(),
        backup: target.exists().then(|| operation.join("previous")),
        previous_sha256: if target.exists() {
            Some(tree_sha256(&target)?)
        } else {
            None
        },
        package_sha256: tree_sha256(&staging)?,
        binary_sha256: request.binary_sha256.clone(),
        binary_relative_path: request.binary_relative_path.clone(),
        version: request.version.clone(),
        evidence: request.evidence.clone(),
        phase: "staged".into(),
        receipt_path: operation.join("receipt.json"),
    };
    write_receipt(&receipt)?;
    // Preparation can take time. Do not activate a candidate if its tested
    // default-branch base advanced while its package was being staged.
    skills_manifest.verify_skills(remote)?;
    remote.verify_remote_binding(upstream)?;
    if let Some(backup) = &receipt.backup {
        fs::rename(&target, backup)
            .context("backup and skills directory must be on the same filesystem")?;
        receipt.phase = "previous_backed_up".into();
        write_receipt(&receipt)?;
    }
    if let Err(error) = fs::rename(&staging, &target) {
        if let Some(backup) = &receipt.backup {
            fs::rename(backup, &target)?;
        }
        bail!("candidate activation failed; previous installation restored: {error}");
    }
    receipt.phase = "activated".into();
    write_receipt(&receipt)?;
    let smoke = capture_text(
        target
            .join(&request.binary_relative_path)
            .to_str()
            .context("non-UTF-8 binary path")?,
        &request.smoke_arguments,
        &target,
    );
    if let Err(error) = smoke {
        rollback_after_smoke(&mut receipt)?;
        bail!("candidate smoke test failed and was rolled back: {error}");
    }
    if file_sha256(&target.join(&request.binary_relative_path))? != request.binary_sha256
        || tree_sha256(&target)? != receipt.package_sha256
    {
        rollback_after_smoke(&mut receipt)?;
        bail!("candidate changed its installed files during smoke testing and was rolled back");
    }
    if let Err(error) = remote.verify_remote_binding(upstream) {
        rollback_after_smoke(&mut receipt)?;
        bail!(
            "upstream changed or could not be verified during installation; candidate rolled back: {error}"
        );
    }
    receipt.phase = "verified".into();
    write_receipt(&receipt)?;
    Ok(receipt)
}

fn rollback_after_smoke(receipt: &mut InstallReceipt) -> Result<()> {
    // The install lock is still held and this is the just-started candidate; preserve any
    // files it wrote as rejected evidence while recovering the verified previous directory.
    rollback_checked(receipt, false)
}

fn rollback_locked(receipt: &mut InstallReceipt) -> Result<()> {
    rollback_checked(receipt, true)
}

fn rollback_checked(receipt: &mut InstallReceipt, verify_active: bool) -> Result<()> {
    if receipt.phase == "rolled_back" {
        return Ok(());
    }
    if receipt.target.exists() && verify_active {
        let active = tree_sha256(&receipt.target)?;
        if Some(&active) == receipt.previous_sha256.as_ref() {
            // Either activation never started, or restore completed before the journal write.
            receipt.phase = "rolled_back".into();
            return write_receipt(receipt);
        }
        ensure!(
            active == receipt.package_sha256,
            "installed files changed; refusing to overwrite another version"
        );
    }
    if let Some(backup) = &receipt.backup {
        ensure!(
            Some(tree_sha256(backup)?) == receipt.previous_sha256,
            "previous installation backup changed"
        );
    }
    let failed = receipt
        .receipt_path
        .parent()
        .context("missing receipt directory")?
        .join("rejected");
    if receipt.target.exists() {
        fs::rename(&receipt.target, &failed)?;
    }
    if let Some(backup) = &receipt.backup {
        fs::rename(backup, &receipt.target)?;
    }
    receipt.phase = "rolled_back".into();
    write_receipt(receipt)
}

/// Read the durable receipt from disk; refuse rollback if a later installation changed the target.
pub fn rollback_install(receipt_path: &Path) -> Result<InstallReceipt> {
    let mut receipt: InstallReceipt = serde_json::from_slice(&fs::read(receipt_path)?)?;
    ensure!(
        fs::canonicalize(receipt_path)? == fs::canonicalize(&receipt.receipt_path)?,
        "receipt path mismatch"
    );
    let root = receipt_path
        .parent()
        .and_then(Path::parent)
        .context("invalid receipt location")?;
    let name = receipt
        .target
        .file_name()
        .and_then(|v| v.to_str())
        .context("invalid installation name")?;
    let _lock = installation_lock(root, name)?;
    rollback_locked(&mut receipt)?;
    Ok(receipt)
}

pub fn capture_text(program: &str, args: &[String], cwd: &Path) -> Result<String> {
    let output = process::capture(program, args, cwd, TIMEOUT)?;
    ensure!(
        output.status.success(),
        "{program} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PullRequestRequest {
    pub repository: String,
    pub working_directory: PathBuf,
    pub branch: String,
    pub base: String,
    pub expected_head: String,
    pub title: String,
    pub body: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PullRequestReceipt {
    pub number: u64,
    pub url: String,
    pub state: String,
    pub head: String,
    pub reused: bool,
}

fn validate_repository(repository: &str) -> Result<(&str, &str)> {
    let (owner, name) = repository
        .split_once('/')
        .context("repository must be OWNER/REPO")?;
    validate_identifier(owner)?;
    // GitHub repositories also permit dots.
    ensure!(
        !name.is_empty()
            && name
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c)),
        "invalid repository name"
    );
    Ok((owner, name))
}

fn gh_json(args: Vec<String>, cwd: &Path) -> Result<Value> {
    serde_json::from_str(&capture_text("gh", &args, cwd)?).context("invalid GitHub JSON response")
}

fn pr_receipt(value: &Value, reused: bool) -> Result<PullRequestReceipt> {
    Ok(PullRequestReceipt {
        number: value["number"].as_u64().context("missing PR number")?,
        url: value["url"].as_str().context("missing PR URL")?.to_owned(),
        state: value["state"]
            .as_str()
            .context("missing PR state")?
            .to_owned(),
        head: value["headRefOid"]
            .as_str()
            .context("missing PR head")?
            .to_owned(),
        reused,
    })
}

/// The branch must already be pushed. A retry reuses its matching PR, including an already merged PR.
pub fn ensure_pull_request(request: &PullRequestRequest) -> Result<PullRequestReceipt> {
    validate_repository(&request.repository)?;
    validate_commit(&request.expected_head)?;
    for branch in [&request.branch, &request.base] {
        capture_text(
            "git",
            &["check-ref-format".into(), "--branch".into(), branch.clone()],
            &request.working_directory,
        )?;
    }
    ensure!(
        request.branch != request.base,
        "PR head and base must differ"
    );
    let values = gh_json(
        vec![
            "pr".into(),
            "list".into(),
            "--repo".into(),
            request.repository.clone(),
            "--head".into(),
            request.branch.clone(),
            "--base".into(),
            request.base.clone(),
            "--state".into(),
            "all".into(),
            "--json".into(),
            "number,url,state,headRefOid".into(),
            "--limit".into(),
            "100".into(),
        ],
        &request.working_directory,
    )?;
    let values = values.as_array().context("expected PR array")?;
    ensure!(
        values.len() < 100,
        "PR lookup is truncated; reconcile before creating another PR"
    );
    let matching: Vec<_> = values
        .iter()
        .filter(|v| v["headRefOid"].as_str() == Some(&request.expected_head))
        .collect();
    ensure!(
        matching.len() <= 1,
        "multiple PRs match the candidate; reconciliation required"
    );
    if let Some(value) = matching.first() {
        let receipt = pr_receipt(value, true)?;
        ensure!(
            receipt.state != "CLOSED",
            "candidate PR was closed; do not recreate automatically"
        );
        return Ok(receipt);
    }
    ensure!(
        !values
            .iter()
            .any(|v| v["state"] == contracts::PullRequestState::Open.as_str()),
        "branch has an open PR at another head; evidence must be refreshed"
    );
    let reference = gh_json(
        vec![
            "api".into(),
            format!(
                "repos/{}/git/ref/heads/{}",
                request.repository, request.branch
            ),
        ],
        &request.working_directory,
    )?;
    ensure!(
        reference["object"]["sha"].as_str() == Some(&request.expected_head),
        "remote branch head differs from evaluated source"
    );
    let body_path = std::env::temp_dir().join(format!("agent-research-pr-{}.md", unique_id()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&body_path)?;
    file.write_all(request.body.as_bytes())?;
    let result = capture_text(
        "gh",
        &[
            "pr".into(),
            "create".into(),
            "--repo".into(),
            request.repository.clone(),
            "--head".into(),
            request.branch.clone(),
            "--base".into(),
            request.base.clone(),
            "--title".into(),
            request.title.clone(),
            "--body-file".into(),
            body_path.to_string_lossy().into_owned(),
        ],
        &request.working_directory,
    );
    let _ = fs::remove_file(body_path);
    let url = result?;
    let value = gh_json(
        vec![
            "pr".into(),
            "view".into(),
            url,
            "--repo".into(),
            request.repository.clone(),
            "--json".into(),
            "number,url,state,headRefOid".into(),
        ],
        &request.working_directory,
    )?;
    let receipt = pr_receipt(&value, false)?;
    ensure!(
        receipt.head == request.expected_head,
        "PR head moved while creating PR; re-evaluate before merging"
    );
    Ok(receipt)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MergeRequest {
    pub repository: String,
    pub working_directory: PathBuf,
    pub number: u64,
    pub expected_head: String,
    pub evidence: ReleaseEvidence,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReviewRecord {
    pub author: String,
    pub state: String,
    pub commit: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MergeSnapshot {
    pub head: String,
    pub base_ref: String,
    pub base_commit: String,
    pub default_branch: String,
    pub default_commit: String,
    pub state: String,
    pub draft: bool,
    pub author: String,
    pub mergeable: String,
    pub merge_state: String,
    pub review_decision: Option<String>,
    pub checks: Vec<String>,
    pub reviews: Vec<ReviewRecord>,
    pub unresolved_threads: usize,
    pub complete: bool,
}

/// Missing, stale, queued, skipped, truncated and unknown evidence is never a pass.
pub fn evaluate_merge_gate(request: &MergeRequest, snapshot: &MergeSnapshot) -> Result<()> {
    request.evidence.validate()?;
    let upstream = request
        .evidence
        .upstream
        .as_ref()
        .context("missing release upstream baseline")?;
    ensure!(
        request.repository == upstream.repository,
        "release evidence belongs to a different repository"
    );
    ensure!(
        snapshot.base_ref == upstream.default_branch
            && snapshot.default_branch == upstream.default_branch
            && snapshot.base_commit == upstream.commit
            && snapshot.default_commit == upstream.commit,
        "upstream default branch advanced or PR targets another base; rebase and re-evaluate before merging"
    );
    ensure!(
        request.expected_head == request.evidence.source_commit
            && snapshot.head == request.expected_head,
        "PR head does not match evaluated source"
    );
    ensure!(snapshot.complete, "GitHub evidence is missing or truncated");
    ensure!(
        snapshot.state == contracts::PullRequestState::Open.as_str() && !snapshot.draft,
        "PR must be open and ready"
    );
    ensure!(
        snapshot.mergeable == contracts::MergeabilityState::Mergeable.as_str()
            && snapshot.merge_state == contracts::MergeQueueState::Clean.as_str(),
        "GitHub merge requirements are not satisfied"
    );
    ensure!(
        !snapshot.checks.is_empty()
            && snapshot
                .checks
                .iter()
                .all(|check| check == contracts::CheckConclusion::Success.as_str()),
        "all CI checks must have succeeded"
    );
    ensure!(
        snapshot.unresolved_threads == 0,
        "unresolved review threads remain"
    );
    ensure!(
        snapshot
            .review_decision
            .as_deref()
            .is_none_or(|value| value == contracts::ReviewState::Approved.as_str()),
        "required GitHub approvals are missing"
    );
    let mut latest = BTreeMap::new();
    for review in &snapshot.reviews {
        if review.state == contracts::ReviewState::Approved.as_str()
            || review.state == contracts::ReviewState::ChangesRequested.as_str()
            || review.state == contracts::ReviewState::Dismissed.as_str()
        {
            latest.insert(&review.author, review);
        }
    }
    ensure!(
        !latest
            .values()
            .any(|r| r.state == contracts::ReviewState::ChangesRequested.as_str()),
        "changes have been requested"
    );
    // reviewDecision is GitHub's aggregate of the repository's required approval policy.
    // Repositories without required reviewers expose null; independent evaluation remains mandatory.
    Ok(())
}

fn github_snapshot(request: &MergeRequest) -> Result<MergeSnapshot> {
    let (owner, name) = validate_repository(&request.repository)?;
    let query = "query($owner:String!,$name:String!,$number:Int!){repository(owner:$owner,name:$name){defaultBranchRef{name target{... on Commit{oid}}} pullRequest(number:$number){headRefOid baseRefName baseRefOid state isDraft author{login} mergeable mergeStateStatus reviewDecision commits(last:1){nodes{commit{oid statusCheckRollup{contexts(first:100){nodes{__typename ... on CheckRun{status conclusion} ... on StatusContext{state}} pageInfo{hasNextPage}}}}}} reviews(first:100){nodes{author{login} state commit{oid}} pageInfo{hasNextPage}} reviewThreads(first:100){nodes{isResolved} pageInfo{hasNextPage}}}}}";
    let value = gh_json(
        vec![
            "api".into(),
            "graphql".into(),
            "-f".into(),
            format!("query={query}"),
            "-f".into(),
            format!("owner={owner}"),
            "-f".into(),
            format!("name={name}"),
            "-F".into(),
            format!("number={}", request.number),
        ],
        &request.working_directory,
    )?;
    ensure!(
        value.get("errors").is_none(),
        "GitHub returned GraphQL errors"
    );
    let pr = &value["data"]["repository"]["pullRequest"];
    let commit = &pr["commits"]["nodes"][0]["commit"];
    let contexts = &commit["statusCheckRollup"]["contexts"];
    let reviews = &pr["reviews"];
    let threads = &pr["reviewThreads"];
    let complete = [contexts, reviews, threads]
        .iter()
        .all(|v| v["pageInfo"]["hasNextPage"] == false && v["nodes"].is_array())
        && commit["oid"] == pr["headRefOid"];
    let read = |key: &str| -> Result<String> {
        Ok(pr[key]
            .as_str()
            .with_context(|| format!("missing PR field {key}"))?
            .to_owned())
    };
    Ok(MergeSnapshot {
        head: read("headRefOid")?,
        base_ref: read("baseRefName")?,
        base_commit: read("baseRefOid")?,
        default_branch: value["data"]["repository"]["defaultBranchRef"]["name"]
            .as_str()
            .context("missing repository default branch")?
            .into(),
        default_commit: value["data"]["repository"]["defaultBranchRef"]["target"]["oid"]
            .as_str()
            .context("missing repository default commit")?
            .into(),
        state: read("state")?,
        draft: pr["isDraft"].as_bool().context("missing draft state")?,
        author: pr["author"]["login"]
            .as_str()
            .context("missing author")?
            .into(),
        mergeable: read("mergeable")?,
        merge_state: read("mergeStateStatus")?,
        review_decision: pr["reviewDecision"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(String::from),
        complete,
        checks: contexts["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|c| {
                if c["__typename"] == "CheckRun" {
                    if c["status"] == contracts::CheckState::Completed.as_str() {
                        c["conclusion"]
                            .as_str()
                            .unwrap_or(contracts::CheckConclusion::Unknown.as_str())
                            .into()
                    } else {
                        contracts::CheckConclusion::Pending.to_string()
                    }
                } else if c["__typename"] == "StatusContext" {
                    c["state"]
                        .as_str()
                        .unwrap_or(contracts::CheckConclusion::Unknown.as_str())
                        .into()
                } else {
                    contracts::CheckConclusion::Unknown.to_string()
                }
            })
            .collect(),
        reviews: reviews["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|r| ReviewRecord {
                author: r["author"]["login"].as_str().unwrap_or("").into(),
                state: r["state"]
                    .as_str()
                    .unwrap_or(contracts::CheckConclusion::Unknown.as_str())
                    .into(),
                commit: r["commit"]["oid"].as_str().unwrap_or("").into(),
            })
            .collect(),
        unresolved_threads: threads["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|t| t["isResolved"] != true)
            .count(),
    })
}

pub fn merge_pull_request(request: &MergeRequest) -> Result<PullRequestReceipt> {
    validate_commit(&request.expected_head)?;
    let snapshot = github_snapshot(request)?;
    if snapshot.state == contracts::PullRequestState::Merged.as_str() {
        ensure!(
            snapshot.head == request.expected_head,
            "already merged PR has another head"
        );
        let value = gh_json(
            vec![
                "pr".into(),
                "view".into(),
                request.number.to_string(),
                "--repo".into(),
                request.repository.clone(),
                "--json".into(),
                "number,url,state,headRefOid".into(),
            ],
            &request.working_directory,
        )?;
        return pr_receipt(&value, true);
    }
    evaluate_merge_gate(request, &snapshot)?;
    request
        .evidence
        .skills_manifest
        .as_ref()
        .context("missing release skills manifest")?
        .verify_skills(&RemoteClient::default())?;
    // Skill checks can be slow. Refresh PR/base/check state after them, then
    // perform the latest default-ref check immediately before requesting merge.
    let final_snapshot = github_snapshot(request)?;
    evaluate_merge_gate(request, &final_snapshot)?;
    freshness::verify_remote_binding(
        request
            .evidence
            .upstream
            .as_ref()
            .context("missing release upstream baseline")?,
    )?;
    capture_text(
        "gh",
        &[
            "pr".into(),
            "merge".into(),
            request.number.to_string(),
            "--repo".into(),
            request.repository.clone(),
            "--squash".into(),
            "--match-head-commit".into(),
            request.expected_head.clone(),
        ],
        &request.working_directory,
    )?;
    // GitHub may enqueue a merge. Return the actual state rather than claiming completion.
    let value = gh_json(
        vec![
            "pr".into(),
            "view".into(),
            request.number.to_string(),
            "--repo".into(),
            request.repository.clone(),
            "--json".into(),
            "number,url,state,headRefOid".into(),
        ],
        &request.working_directory,
    )?;
    let receipt = pr_receipt(&value, true)?;
    ensure!(
        receipt.head == request.expected_head,
        "head changed during merge; inspect GitHub before retrying"
    );
    Ok(receipt)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn upstream() -> RemoteSnapshot {
        RemoteSnapshot {
            repository: "coolplayagent/example".into(),
            default_branch: "main".into(),
            commit: "0".repeat(40),
            checked_at: "2026-10-09T00:00:00Z".into(),
        }
    }

    #[cfg(unix)]
    fn mock_remote(root: &Path) -> RemoteClient {
        use std::os::unix::fs::PermissionsExt;
        let head = root.join("current-upstream.json");
        fs::write(
            &head,
            serde_json::to_vec(&json!({"sha":upstream().commit})).unwrap(),
        )
        .unwrap();
        let gh = root.join("fake-gh");
        let quoted = head.to_string_lossy().replace('\'', "'\\''");
        let release = root.join("current-release.json");
        if !release.exists() {
            fs::write(
                &release,
                r#"{"id":1,"tag_name":"v1","draft":false,"prerelease":false}"#,
            )
            .unwrap();
        }
        let release = release.to_string_lossy().replace('\'', "'\\''");
        fs::write(&gh, format!("#!/bin/sh\nset -eu\ncase \"$1\" in\nrepo) printf '%s\\n' '{{\"nameWithOwner\":\"coolplayagent/example\",\"defaultBranchRef\":{{\"name\":\"main\"}}}}';;\napi) case \"$2\" in */releases/latest) cat '{release}';; *) cat '{quoted}';; esac;;\n*) exit 92;;\nesac\n")).unwrap();
        fs::set_permissions(&gh, fs::Permissions::from_mode(0o755)).unwrap();
        RemoteClient {
            git: PathBuf::from("git"),
            gh,
            timeout: Duration::from_secs(2),
        }
    }

    #[cfg(unix)]
    fn install_fixture(request: &InstallRequest) -> Result<InstallReceipt> {
        install_candidate_with_client(
            request,
            &mock_remote(request.skill_source.parent().unwrap()),
        )
    }

    fn artifact(root: &Path, name: &str, value: Value) -> VerificationArtifact {
        let path = root.join(name);
        fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        VerificationArtifact {
            sha256: file_sha256(&path).unwrap(),
            path,
        }
    }

    fn evidence(root: &Path) -> ReleaseEvidence {
        let installed = root.join("verified-skill");
        fs::create_dir_all(&installed).unwrap();
        fs::write(
            installed.join("SKILL.md"),
            "---\nname: verified-skill\n---\nFixture",
        )
        .unwrap();
        fs::write(installed.join("cli"), "verified runtime").unwrap();
        let skills_manifest = artifact(
            root,
            "skills-manifest.json",
            json!({"schema_version":1,"skills":[{"name":"verified-skill","repository":"coolplayagent/example","installed_path":installed,"runtime_path":installed.join("cli"),"tree_sha256":freshness::skill_tree_digest(&installed).unwrap(),"runtime_sha256":file_sha256(&installed.join("cli")).unwrap(),"release_tag":"v1","release_id":1,"source_commit":null,"checked_at":"2026-10-09T00:00:00Z"}]}),
        );
        let qualitygate_report = artifact(
            root,
            "qualitygate.json",
            json!({ "profile": "full", "scope": "delivery", "gate": {"complete":true,"decision":"pass"}, "plan":{"pending_delivery_checks":[]},"snapshot":{"content_digest":"e".repeat(64),"verification_digest":"f".repeat(64)} }),
        );
        let snapshot_receipt = artifact(
            root,
            "snapshot.json",
            json!({"source_commit":"a".repeat(40),"upstream":upstream(),"skills_manifest_sha256":skills_manifest.sha256,"prompt_sha256":"b".repeat(64),"policy_sha256":"c".repeat(64),"superpod_commit":"d".repeat(40),"qualitygate_report_sha256":qualitygate_report.sha256,"content_digest":"e".repeat(64),"verification_digest":"f".repeat(64)}),
        );
        let evaluation_receipt = artifact(
            root,
            "evaluation.json",
            json!({"role":"independent_evaluator","status":"pass","complete":true,"source_commit":"a".repeat(40),"upstream":upstream(),"skills_manifest_sha256":skills_manifest.sha256,"prompt_sha256":"b".repeat(64),"policy_sha256":"c".repeat(64),"superpod_commit":"d".repeat(40),"evaluator_id":"evaluation","implementer_id":"implementation"}),
        );
        ReleaseEvidence {
            source_commit: "a".repeat(40),
            upstream: Some(upstream()),
            skills_manifest: Some(skills_manifest),
            prompt_sha256: "b".repeat(64),
            policy_sha256: "c".repeat(64),
            superpod_commit: "d".repeat(40),
            implementer_id: "implementation".into(),
            evaluator_id: "evaluation".into(),
            evaluation_passed: true,
            qualitygate_passed: true,
            qualitygate_report,
            snapshot_receipt,
            evaluation_receipt,
        }
    }

    #[test]
    fn merge_rejects_stale_unknown_failed_and_unresolved_evidence() {
        let temp = tempfile::tempdir().unwrap();
        let request = MergeRequest {
            repository: "coolplayagent/example".into(),
            working_directory: PathBuf::from("."),
            number: 1,
            expected_head: "a".repeat(40),
            evidence: evidence(temp.path()),
        };
        let valid = MergeSnapshot {
            head: request.expected_head.clone(),
            base_ref: "main".into(),
            base_commit: upstream().commit.clone(),
            default_branch: "main".into(),
            default_commit: upstream().commit,
            state: "OPEN".into(),
            draft: false,
            author: "author".into(),
            mergeable: "MERGEABLE".into(),
            merge_state: "CLEAN".into(),
            review_decision: Some("APPROVED".into()),
            checks: vec!["SUCCESS".into()],
            reviews: vec![ReviewRecord {
                author: "reviewer".into(),
                state: "APPROVED".into(),
                commit: request.expected_head.clone(),
            }],
            unresolved_threads: 0,
            complete: true,
        };
        evaluate_merge_gate(&request, &valid).unwrap();
        let mut stale = valid.clone();
        stale.head = "e".repeat(40);
        assert!(evaluate_merge_gate(&request, &stale).is_err());
        let mut advanced = valid.clone();
        advanced.base_commit = "1".repeat(40);
        advanced.default_commit = advanced.base_commit.clone();
        assert!(evaluate_merge_gate(&request, &advanced).is_err());
        let mut retargeted = valid.clone();
        retargeted.base_ref = "old-release".into();
        assert!(evaluate_merge_gate(&request, &retargeted).is_err());
        let mut incomplete = valid.clone();
        incomplete.complete = false;
        assert!(evaluate_merge_gate(&request, &incomplete).is_err());
        let mut pending = valid.clone();
        pending.checks = vec!["PENDING".into()];
        assert!(evaluate_merge_gate(&request, &pending).is_err());
        let mut unresolved = valid.clone();
        unresolved.unresolved_threads = 1;
        assert!(evaluate_merge_gate(&request, &unresolved).is_err());
        let mut missing_review = valid.clone();
        missing_review.review_decision = Some("REVIEW_REQUIRED".into());
        assert!(evaluate_merge_gate(&request, &missing_review).is_err());
        let mut no_required_review = valid;
        no_required_review.reviews.clear();
        no_required_review.review_decision = None;
        evaluate_merge_gate(&request, &no_required_review).unwrap();
    }

    #[cfg(unix)]
    fn fixture(failing: bool) -> (tempfile::TempDir, InstallRequest) {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("candidate");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("SKILL.md"), "---\nname: test-skill\n---\nTest").unwrap();
        fs::write(
            source.join("cli"),
            if failing {
                "#!/bin/sh\nexit 7\n"
            } else {
                "#!/bin/sh\nprintf 'version 1\\n'\n"
            },
        )
        .unwrap();
        fs::set_permissions(source.join("cli"), fs::Permissions::from_mode(0o755)).unwrap();
        let mut evidence = evidence(temp.path());
        let mut snapshot = evidence.snapshot_receipt.read().unwrap();
        snapshot["package_sha256"] = tree_sha256(&source).unwrap().into();
        snapshot["binary_sha256"] = file_sha256(&source.join("cli")).unwrap().into();
        evidence.snapshot_receipt = artifact(temp.path(), "snapshot.json", snapshot);
        let request = InstallRequest {
            skill_source: source.clone(),
            skills_root: temp.path().join("skills"),
            backup_root: temp.path().join("backups"),
            skill_directory_name: "test-skill".into(),
            binary_relative_path: "cli".into(),
            binary_sha256: file_sha256(&source.join("cli")).unwrap(),
            version: "1-dev".into(),
            evidence,
            smoke_arguments: vec!["--version".into()],
        };
        (temp, request)
    }

    #[cfg(unix)]
    #[test]
    fn installation_and_rollback_preserve_previous_version() {
        let (_temp, request) = fixture(false);
        let previous = request.skills_root.join("test-skill");
        fs::create_dir_all(&previous).unwrap();
        fs::write(previous.join("SKILL.md"), "---\nname: test-skill\n---\nOld").unwrap();
        let receipt = install_fixture(&request).unwrap();
        assert_eq!(receipt.phase, "verified");
        assert_eq!(
            install_fixture(&request).unwrap().operation_id,
            receipt.operation_id
        );
        assert!(receipt.target.join("cli").exists());
        let rolled = rollback_install(&receipt.receipt_path).unwrap();
        assert_eq!(rolled.phase, "rolled_back");
        assert!(
            fs::read_to_string(previous.join("SKILL.md"))
                .unwrap()
                .ends_with("Old")
        );
        assert_eq!(
            rollback_install(&receipt.receipt_path).unwrap().phase,
            "rolled_back"
        );
    }

    #[cfg(unix)]
    #[test]
    fn failed_smoke_rolls_back_and_wrong_hash_never_activates() {
        let (_temp, mut request) = fixture(true);
        assert!(install_fixture(&request).is_err());
        assert!(!request.skills_root.join("test-skill").exists());
        request.binary_sha256 = "0".repeat(64);
        assert!(install_fixture(&request).is_err());
        assert!(!request.skills_root.join("test-skill").exists());
    }

    #[cfg(unix)]
    #[test]
    fn interrupted_backup_is_recovered_before_reinstall() {
        let (_temp, request) = fixture(false);
        let previous = request.skills_root.join("test-skill");
        fs::create_dir_all(&previous).unwrap();
        fs::write(previous.join("SKILL.md"), "---\nname: test-skill\n---\nOld").unwrap();
        let mut interrupted = install_fixture(&request).unwrap();
        let staged = interrupted.receipt_path.parent().unwrap().join("candidate");
        fs::rename(&interrupted.target, staged).unwrap();
        interrupted.phase = "previous_backed_up".into();
        write_receipt(&interrupted).unwrap();
        let replacement = install_fixture(&request).unwrap();
        assert_eq!(replacement.phase, "verified");
        assert_eq!(
            fs::read_to_string(replacement.backup.unwrap().join("SKILL.md")).unwrap(),
            "---\nname: test-skill\n---\nOld"
        );
    }

    #[cfg(unix)]
    #[test]
    fn rollback_refuses_to_overwrite_a_later_modified_installation() {
        let (_temp, request) = fixture(false);
        let receipt = install_fixture(&request).unwrap();
        fs::write(receipt.target.join("newer-file"), "later change").unwrap();
        assert!(rollback_install(&receipt.receipt_path).is_err());
        assert!(receipt.target.join("newer-file").exists());
    }

    #[test]
    fn booleans_do_not_override_tampered_or_stale_artifacts() {
        let temp = tempfile::tempdir().unwrap();
        let mut evidence = evidence(temp.path());
        evidence.validate().unwrap();
        fs::write(&evidence.qualitygate_report.path, "{}").unwrap();
        assert!(evidence.validate().is_err());
        evidence.qualitygate_report.sha256 =
            file_sha256(&evidence.qualitygate_report.path).unwrap();
        assert!(evidence.validate().is_err());
    }

    #[test]
    fn historical_evidence_can_be_read_but_cannot_pass_new_release_gates() {
        let temp = tempfile::tempdir().unwrap();
        let mut old = serde_json::to_value(evidence(temp.path())).unwrap();
        old.as_object_mut().unwrap().remove("upstream");
        let old: ReleaseEvidence = serde_json::from_value(old).unwrap();
        assert!(old.upstream.is_none());
        assert!(old.validate().is_err());
        let mut without_skills = serde_json::to_value(evidence(temp.path())).unwrap();
        without_skills
            .as_object_mut()
            .unwrap()
            .remove("skills_manifest");
        let without_skills: ReleaseEvidence = serde_json::from_value(without_skills).unwrap();
        assert!(without_skills.validate().is_err());
        let mut mismatched = evidence(temp.path());
        mismatched.upstream.as_mut().unwrap().commit = "1".repeat(40);
        assert!(mismatched.validate().is_err());
    }

    #[cfg(unix)]
    #[test]
    fn installation_rejects_advanced_or_unverifiable_upstream_before_activation() {
        let (temp, request) = fixture(false);
        let client = mock_remote(temp.path());
        fs::write(
            temp.path().join("current-upstream.json"),
            serde_json::to_vec(&json!({"sha":"1".repeat(40)})).unwrap(),
        )
        .unwrap();
        assert!(install_candidate_with_client(&request, &client).is_err());
        assert!(!request.skills_root.join("test-skill").exists());
        fs::write(temp.path().join("current-upstream.json"), "{}").unwrap();
        assert!(install_candidate_with_client(&request, &client).is_err());
        assert!(!request.skills_root.join("test-skill").exists());
        fs::write(
            temp.path().join("current-upstream.json"),
            serde_json::to_vec(&json!({"sha":upstream().commit})).unwrap(),
        )
        .unwrap();
        fs::write(
            temp.path().join("current-release.json"),
            r#"{"id":2,"tag_name":"v2","draft":false,"prerelease":false}"#,
        )
        .unwrap();
        assert!(install_candidate_with_client(&request, &client).is_err());
        assert!(!request.skills_root.join("test-skill").exists());
        fs::write(
            &request.evidence.skills_manifest.as_ref().unwrap().path,
            "{}",
        )
        .unwrap();
        assert!(install_candidate_with_client(&request, &client).is_err());
        assert!(!request.skills_root.join("test-skill").exists());
    }
}
