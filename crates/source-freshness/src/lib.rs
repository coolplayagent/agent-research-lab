//! Host-controlled freshness gates. A network or validation failure never permits
//! an older local checkout, skill installation, or release identity as a fallback.
use anyhow::{Context, Result, bail, ensure};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
    time::Duration,
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RemoteSnapshot {
    pub repository: String,
    pub default_branch: String,
    pub commit: String,
    pub checked_at: String,
}
impl RemoteSnapshot {
    pub fn validate(&self) -> Result<()> {
        trusted_repository(&self.repository)?;
        validate_branch(&self.default_branch)?;
        validate_hex(&self.commit, 40)?;
        chrono::DateTime::parse_from_rfc3339(&self.checked_at)
            .context("invalid freshness timestamp")?;
        Ok(())
    }
    pub fn same_binding(&self, other: &Self) -> bool {
        self.repository.eq_ignore_ascii_case(&other.repository)
            && self.default_branch == other.default_branch
            && self.commit == other.commit
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillSnapshot {
    pub name: String,
    pub repository: String,
    pub installed_path: PathBuf,
    pub runtime_path: PathBuf,
    pub tree_sha256: String,
    pub runtime_sha256: String,
    pub release_tag: Option<String>,
    pub release_id: Option<u64>,
    pub source_commit: Option<String>,
    /// A locally built current default-branch skill may supersede a release only
    /// with a host-retained, passing full gate on that exact source commit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_qualitygate: Option<SourceQualitygate>,
    pub checked_at: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceQualitygate {
    pub path: PathBuf,
    pub sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillsManifest {
    pub schema_version: u32,
    pub skills: Vec<SkillSnapshot>,
}

#[derive(Clone, Debug)]
pub struct RemoteClient {
    pub git: PathBuf,
    pub gh: PathBuf,
    pub timeout: Duration,
}
impl Default for RemoteClient {
    fn default() -> Self {
        Self {
            git: "git".into(),
            gh: "gh".into(),
            timeout: Duration::from_secs(90),
        }
    }
}
impl RemoteClient {
    fn command(
        &self,
        binary: &Path,
        arguments: &[String],
        cwd: &Path,
    ) -> Result<std::process::Output> {
        process::capture(
            binary.to_str().context("non-UTF8 tool path")?,
            arguments,
            cwd,
            self.timeout,
        )
    }
    fn checked(&self, binary: &Path, arguments: &[String], cwd: &Path) -> Result<String> {
        let output = self.command(binary, arguments, cwd)?;
        ensure!(
            output.status.success(),
            "freshness command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(String::from_utf8(output.stdout)?.trim().to_owned())
    }
    fn gh_json(&self, arguments: &[String]) -> Result<Value> {
        let output = self.checked(&self.gh, arguments, &std::env::current_dir()?)?;
        serde_json::from_str(&output).context("GitHub freshness response was not JSON")
    }
    fn default_branch(&self, repository: &str) -> Result<(String, String)> {
        trusted_repository(repository)?;
        let value = self.gh_json(&[
            "repo".into(),
            "view".into(),
            repository.into(),
            "--json".into(),
            "nameWithOwner,defaultBranchRef".into(),
        ])?;
        let actual = value["nameWithOwner"]
            .as_str()
            .context("GitHub repository identity missing")?;
        ensure!(
            actual.eq_ignore_ascii_case(repository),
            "GitHub returned another repository"
        );
        trusted_repository(actual)?;
        let branch = value["defaultBranchRef"]["name"]
            .as_str()
            .context("repository has no default branch")?;
        validate_branch(branch)?;
        Ok((actual.into(), branch.into()))
    }
    fn remote_binding(&self, repository: &str) -> Result<RemoteSnapshot> {
        let (repository, branch) = self.default_branch(repository)?;
        let value = self.gh_json(&[
            "api".into(),
            format!("repos/{repository}/commits/{}", url_component(&branch)),
            "--jq".into(),
            "{sha:.sha}".into(),
        ])?;
        let commit = value["sha"]
            .as_str()
            .context("GitHub default branch commit missing")?;
        validate_hex(commit, 40)?;
        Ok(RemoteSnapshot {
            repository,
            default_branch: branch,
            commit: commit.into(),
            checked_at: Utc::now().to_rfc3339(),
        })
    }
    /// Query GitHub, then fetch precisely that default branch into a dedicated
    /// host ref. Neither the checked-out branch nor the worktree/index changes.
    pub fn remote_snapshot(&self, repository_path: &Path) -> Result<RemoteSnapshot> {
        let origin = self.checked(
            &self.git,
            &["config".into(), "--get".into(), "remote.origin.url".into()],
            repository_path,
        )?;
        let repository = github_origin(&origin)?;
        let snapshot = self.remote_binding(&repository)?;
        let reference = format!(
            "refs/agent-research-lab/default/{}",
            snapshot.default_branch
        );
        self.checked(
            &self.git,
            &["check-ref-format".into(), reference.clone()],
            repository_path,
        )?;
        self.checked(
            &self.git,
            &[
                "fetch".into(),
                "--no-tags".into(),
                "--no-write-fetch-head".into(),
                "origin".into(),
                format!("+refs/heads/{}:{reference}", snapshot.default_branch),
            ],
            repository_path,
        )?;
        let fetched = self.checked(
            &self.git,
            &[
                "rev-parse".into(),
                "--verify".into(),
                format!("{reference}^{{commit}}"),
            ],
            repository_path,
        )?;
        ensure!(
            fetched == snapshot.commit,
            "remote default branch moved during fetch; new snapshot required"
        );
        // Reject a default-branch switch or advance that raced the fetch.
        self.verify_remote_binding(&snapshot)
    }
    pub fn verify_fresh(
        &self,
        repository_path: &Path,
        snapshot: &RemoteSnapshot,
    ) -> Result<RemoteSnapshot> {
        snapshot.validate()?;
        let latest = self.remote_snapshot(repository_path)?;
        ensure!(
            snapshot.same_binding(&latest),
            "snapshot is stale; create a new experiment ID at the current remote default branch"
        );
        Ok(latest)
    }
    /// Read-only GitHub gate for publication/activation callers with no local clone.
    pub fn verify_remote_binding(&self, snapshot: &RemoteSnapshot) -> Result<RemoteSnapshot> {
        snapshot.validate()?;
        let latest = self.remote_binding(&snapshot.repository)?;
        ensure!(
            snapshot.same_binding(&latest),
            "snapshot no longer matches the remote default branch"
        );
        Ok(latest)
    }
    fn latest_release(&self, repository: &str) -> Result<Option<(u64, String)>> {
        trusted_repository(repository)?;
        let result = self.command(
            &self.gh,
            &["api".into(), format!("repos/{repository}/releases/latest")],
            &std::env::current_dir()?,
        )?;
        if !result.status.success() {
            let body = serde_json::from_slice::<Value>(&result.stdout).ok();
            let explicit_404 = body
                .as_ref()
                .is_some_and(|v| v["status"] == "404" || v["status"] == 404)
                && String::from_utf8_lossy(&result.stderr).contains("HTTP 404");
            ensure!(
                explicit_404,
                "cannot verify latest published release: {}",
                String::from_utf8_lossy(&result.stderr)
            );
            // GitHub also uses 404 for absent/inaccessible repositories. Establish
            // that the public repository exists before interpreting no release.
            self.default_branch(repository)?;
            return Ok(None);
        }
        let value: Value = serde_json::from_slice(&result.stdout)?;
        ensure!(
            value["draft"] == false && value["prerelease"] == false,
            "latest release is not a stable published release"
        );
        let id = value["id"]
            .as_u64()
            .filter(|id| *id > 0)
            .context("latest release ID missing")?;
        let tag = value["tag_name"]
            .as_str()
            .filter(|tag| !tag.is_empty())
            .context("latest release tag missing")?;
        Ok(Some((id, tag.into())))
    }
    pub fn verify_skills(&self, manifest_path: &Path) -> Result<SkillsManifest> {
        let mut manifest: SkillsManifest = storage::read(manifest_path)?;
        ensure!(
            manifest.schema_version == 1 && !manifest.skills.is_empty(),
            "invalid or empty skills manifest"
        );
        let mut names = std::collections::BTreeSet::new();
        for skill in &manifest.skills {
            config::safe_id(&skill.name)?;
            ensure!(names.insert(skill.name.clone()), "duplicate skill name");
        }
        manifest.skills = process::parallel_map(&manifest.skills, |skill| {
            let mut skill = skill.clone();
            trusted_repository(&skill.repository)?;
            validate_hex(&skill.tree_sha256, 64)?;
            validate_hex(&skill.runtime_sha256, 64)?;
            chrono::DateTime::parse_from_rfc3339(&skill.checked_at)
                .context("invalid skill freshness timestamp")?;
            ensure!(
                skill.installed_path.is_absolute() && skill.runtime_path.is_absolute(),
                "skill/runtime paths must be absolute"
            );
            ensure!(
                skill.installed_path.join("SKILL.md").is_file(),
                "installed skill lacks SKILL.md"
            );
            ensure!(
                skill_tree_digest(&skill.installed_path)? == skill.tree_sha256,
                "installed skill tree differs from host manifest: {}",
                skill.name
            );
            let (runtime_digest, _) = regular_file_digest(&skill.runtime_path)?;
            ensure!(
                hex(&runtime_digest) == skill.runtime_sha256,
                "installed CLI differs from host manifest: {}",
                skill.name
            );
            match (&skill.release_tag, skill.release_id, &skill.source_commit) {
                (Some(tag), Some(id), None) if !tag.is_empty() && id > 0 => {
                    ensure!(
                        skill.source_qualitygate.is_none(),
                        "release skill cannot carry a source override"
                    );
                    ensure!(
                        self.latest_release(&skill.repository)? == Some((id, tag.clone())),
                        "installed skill is not the latest published release: {}",
                        skill.name
                    );
                }
                (None, None, Some(commit)) => {
                    validate_hex(commit, 40)?;
                    let release_exists = self.latest_release(&skill.repository)?.is_some();
                    if release_exists || skill.source_qualitygate.is_some() {
                        verify_source_qualitygate(&skill)?;
                    }
                    let latest = self.remote_binding(&skill.repository)?;
                    ensure!(
                        latest.commit == *commit,
                        "source-installed skill is stale: {}",
                        skill.name
                    );
                }
                _ => bail!("skill must bind exactly one release (tag+ID) or source commit"),
            }
            skill.checked_at = Utc::now().to_rfc3339();
            Ok(skill)
        })?;
        Ok(manifest)
    }
}

fn verify_source_qualitygate(skill: &SkillSnapshot) -> Result<()> {
    let proof = skill.source_qualitygate.as_ref()
        .context("a published release exists; a source build requires an exact-commit full Qualitygate report")?;
    ensure!(
        proof.path.is_absolute(),
        "source Qualitygate path must be absolute"
    );
    validate_hex(&proof.sha256, 64)?;
    reject_symlink_components(&proof.path)?;
    use std::os::unix::fs::OpenOptionsExt;
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(&proof.path)?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file() && metadata.len() <= 16 * 1024 * 1024,
        "source Qualitygate report must be regular and at most 16 MiB"
    );
    let mut bytes = Vec::new();
    file.take(16 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 == metadata.len(),
        "source Qualitygate report changed size while reading"
    );
    // Hash and parse the exact same bounded read, even if a concurrent host
    // publication subsequently replaces the report pathname.
    ensure!(
        hex(&Sha256::digest(&bytes)) == proof.sha256,
        "source Qualitygate report changed"
    );
    let report: Value = serde_json::from_slice(&bytes)?;
    ensure!(
        report["profile"] == "full"
            && matches!(report["scope"].as_str(), Some("delivery" | "task"))
            && report["gate"]["complete"] == true
            && report["gate"]["decision"] == "pass"
            && report["plan"]["pending_delivery_checks"]
                .as_array()
                .is_some_and(Vec::is_empty)
            && report["selection"]["empty_delivery"] == false
            && report["snapshot"]["mode"] == "diff"
            && report["snapshot"]["head"].as_str() == skill.source_commit.as_deref(),
        "source skill needs a complete, nonempty full diff gate at its exact source commit"
    );
    Ok(())
}

pub fn remote_snapshot(repository_path: &Path) -> Result<RemoteSnapshot> {
    RemoteClient::default().remote_snapshot(repository_path)
}
pub fn verify_fresh(repository_path: &Path, snapshot: &RemoteSnapshot) -> Result<RemoteSnapshot> {
    RemoteClient::default().verify_fresh(repository_path, snapshot)
}
pub fn verify_remote_binding(snapshot: &RemoteSnapshot) -> Result<RemoteSnapshot> {
    RemoteClient::default().verify_remote_binding(snapshot)
}
pub fn verify_skills(manifest_path: &Path) -> Result<SkillsManifest> {
    RemoteClient::default().verify_skills(manifest_path)
}

/// Local boundary check around a model attempt. A concurrent host installation
/// invalidates its observations instead of silently mixing tool versions.
pub fn verify_installed_bytes(manifest: &SkillsManifest) -> Result<()> {
    for skill in &manifest.skills {
        ensure!(
            skill_tree_digest(&skill.installed_path)? == skill.tree_sha256,
            "skill changed during experiment: {}",
            skill.name
        );
        let (digest, _) = regular_file_digest(&skill.runtime_path)?;
        ensure!(
            hex(&digest) == skill.runtime_sha256,
            "CLI changed during experiment: {}",
            skill.name
        );
    }
    Ok(())
}

/// Canonical manifest contract: sorted relative POSIX UTF-8 file paths; hash
/// path || NUL || raw file SHA256 (32 bytes) || newline for every regular file.
/// Empty directories are ignored. Symlinks and special files are rejected.
pub fn skill_tree_digest(root: &Path) -> Result<String> {
    ensure!(root.is_absolute(), "skill tree path must be absolute");
    reject_symlink_components(root)?;
    ensure!(
        fs::metadata(root)?.is_dir(),
        "skill root is not a directory"
    );
    let mut pending = vec![root.to_owned()];
    let mut files = vec![];
    let mut entries = 0usize;
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory)? {
            let path = entry?.path();
            entries += 1;
            ensure!(entries <= 10000, "skill tree exceeds 10000 entries");
            let metadata = fs::symlink_metadata(&path)?;
            ensure!(
                !metadata.file_type().is_symlink(),
                "skill tree contains a symlink"
            );
            if metadata.is_dir() {
                pending.push(path);
            } else {
                ensure!(metadata.is_file(), "skill tree contains a special file");
                let relative = path
                    .strip_prefix(root)?
                    .components()
                    .map(|part| part.as_os_str().to_str().context("skill path is not UTF-8"))
                    .collect::<Result<Vec<_>>>()?
                    .join("/");
                files.push((relative, path));
            }
        }
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    let mut tree = Sha256::new();
    let mut bytes = 0_u64;
    for (relative, file) in files {
        let (digest, size) = regular_file_digest(&file)?;
        bytes = bytes
            .checked_add(size)
            .context("skill tree size overflow")?;
        ensure!(bytes <= 1024 * 1024 * 1024, "skill tree exceeds 1 GiB");
        tree.update(relative.as_bytes());
        tree.update([0]);
        tree.update(digest);
        tree.update(b"\n");
    }
    Ok(hex(&tree.finalize()))
}
fn regular_file_digest(path: &Path) -> Result<([u8; 32], u64)> {
    reject_symlink_components(path)?;
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file() && metadata.len() <= 256 * 1024 * 1024,
        "runtime/skill file must be regular and at most 256 MiB"
    );
    let mut file = fs::File::open(path)?;
    let mut sha = Sha256::new();
    let mut buffer = [0_u8; 65536];
    let mut size = 0_u64;
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        size += count as u64;
        ensure!(size <= 256 * 1024 * 1024, "file grew beyond bound");
        sha.update(&buffer[..count]);
    }
    ensure!(size == metadata.len(), "file changed while hashing");
    Ok((sha.finalize().into(), size))
}
fn reject_symlink_components(path: &Path) -> Result<()> {
    ensure!(path.is_absolute(), "freshness paths must be absolute");
    let mut current = PathBuf::new();
    for component in path.components() {
        ensure!(
            !matches!(component, Component::ParentDir),
            "parent path traversal is not allowed"
        );
        current.push(component.as_os_str());
        ensure!(
            !fs::symlink_metadata(&current)?.file_type().is_symlink(),
            "freshness path traverses a symlink"
        );
    }
    Ok(())
}
fn github_origin(origin: &str) -> Result<String> {
    let repository = origin
        .strip_prefix("https://github.com/")
        .or_else(|| origin.strip_prefix("git@github.com:"))
        .or_else(|| origin.strip_prefix("ssh://git@github.com/"))
        .context("origin must identify an authorized GitHub repository")?;
    let repository = repository
        .trim_end_matches('/')
        .strip_suffix(".git")
        .unwrap_or(repository.trim_end_matches('/'));
    trusted_repository(repository)?;
    Ok(repository.to_owned())
}
fn trusted_repository(repository: &str) -> Result<()> {
    let parts: Vec<_> = repository.split('/').collect();
    ensure!(
        parts.len() == 2
            && parts.iter().all(|s| !s.is_empty()
                && *s != "."
                && *s != ".."
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')),
        "invalid GitHub repository identity"
    );
    ensure!(
        parts[0].eq_ignore_ascii_case("coolplayagent")
            || repository.eq_ignore_ascii_case("stevetdp/superpod"),
        "repository is outside the authorized namespace"
    );
    Ok(())
}
fn validate_hex(value: &str, length: usize) -> Result<()> {
    ensure!(
        value.len() == length
            && value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "invalid immutable digest"
    );
    Ok(())
}
fn validate_branch(branch: &str) -> Result<()> {
    ensure!(
        !branch.is_empty()
            && !branch.starts_with('-')
            && !branch.starts_with('/')
            && !branch.ends_with('/')
            && !branch.contains("..")
            && !branch.contains("@{")
            && !branch.contains("//")
            && !branch.ends_with('.')
            && !branch.ends_with(".lock")
            && !branch
                .chars()
                .any(|c| c.is_control() || " ~^:?*[\\".contains(c)),
        "invalid default branch name"
    );
    Ok(())
}
fn url_component(text: &str) -> String {
    text.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    fn git(cwd: &Path, args: &[&str]) -> String {
        process::checked(
            "git",
            &args.iter().map(|s| (*s).into()).collect::<Vec<_>>(),
            cwd,
        )
        .unwrap()
    }
    fn fixture() -> (tempfile::TempDir, RemoteClient, PathBuf, PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path();
        let remote = base.join("remote.git");
        git(
            base,
            &[
                "init",
                "--bare",
                "--initial-branch=main",
                remote.to_str().unwrap(),
            ],
        );
        let author = base.join("author");
        fs::create_dir(&author).unwrap();
        git(&author, &["init", "--initial-branch=main"]);
        git(&author, &["config", "user.name", "Freshness Fixture"]);
        git(
            &author,
            &["config", "user.email", "fixture@example.invalid"],
        );
        fs::write(author.join("tracked.txt"), "first remote commit").unwrap();
        git(&author, &["add", "tracked.txt"]);
        git(&author, &["commit", "-m", "first"]);
        git(
            &author,
            &["remote", "add", "origin", remote.to_str().unwrap()],
        );
        git(&author, &["push", "origin", "main"]);
        let checkout = base.join("checkout");
        git(
            base,
            &[
                "clone",
                remote.to_str().unwrap(),
                checkout.to_str().unwrap(),
            ],
        );
        git(
            &checkout,
            &[
                "remote",
                "set-url",
                "origin",
                "https://github.com/coolplayagent/freshness-fixture.git",
            ],
        );
        git(
            &checkout,
            &[
                "config",
                &format!("url.{}.insteadOf", remote.display()),
                "https://github.com/coolplayagent/freshness-fixture.git",
            ],
        );
        git(&checkout, &["checkout", "-b", "feature/local-work"]);
        fs::write(checkout.join("tracked.txt"), "uncommitted user work").unwrap();
        fs::write(base.join("default-branch"), "main").unwrap();
        fs::write(
            base.join("release.json"),
            r#"{"id":42,"tag_name":"v1","draft":false,"prerelease":false}"#,
        )
        .unwrap();
        let gh = base.join("fake-gh");
        fs::write(&gh,r#"#!/bin/sh
set -eu
base=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
if [ -e "$base/offline" ]; then printf 'network unavailable\n' >&2; exit 1; fi
branch=$(cat "$base/default-branch")
case "$1 $2" in
  'repo view') printf '{"nameWithOwner":"coolplayagent/freshness-fixture","defaultBranchRef":{"name":"%s"}}\n' "$branch" ;;
  'api repos/coolplayagent/freshness-fixture/releases/latest')
    if [ -e "$base/no-release" ]; then printf '{"status":"404","message":"Not Found"}\n'; printf 'gh: Not Found (HTTP 404)\n' >&2; exit 1; fi
    cat "$base/release.json" ;;
  api*) sha=$(git --git-dir="$base/remote.git" rev-parse "refs/heads/$branch"); printf '{"sha":"%s"}\n' "$sha" ;;
  *) exit 9 ;;
esac
"#).unwrap();
        fs::set_permissions(&gh, fs::Permissions::from_mode(0o700)).unwrap();
        let client = RemoteClient {
            git: "git".into(),
            gh,
            timeout: Duration::from_secs(10),
        };
        (temp, client, checkout, author)
    }
    #[test]
    fn fetches_remote_default_without_touching_feature_head_or_user_changes() {
        let (temp, client, checkout, author) = fixture();
        let local_head = git(&checkout, &["rev-parse", "HEAD"]);
        let local_diff = git(&checkout, &["diff"]);
        let first = client.remote_snapshot(&checkout).unwrap();
        assert_eq!(first.commit, local_head);
        assert_eq!(first.default_branch, "main");
        fs::write(author.join("tracked.txt"), "latest remote work").unwrap();
        git(&author, &["commit", "-am", "advance remote default"]);
        git(&author, &["push", "origin", "main"]);
        assert!(client.verify_fresh(&checkout, &first).is_err());
        assert!(client.verify_remote_binding(&first).is_err());
        let latest = client.remote_snapshot(&checkout).unwrap();
        assert_eq!(latest.commit, git(&author, &["rev-parse", "HEAD"]));
        assert_ne!(latest.commit, first.commit);
        assert_eq!(
            git(&checkout, &["branch", "--show-current"]),
            "feature/local-work"
        );
        assert_eq!(git(&checkout, &["rev-parse", "HEAD"]), local_head);
        assert_eq!(git(&checkout, &["diff"]), local_diff);
        fs::write(temp.path().join("offline"), "").unwrap();
        assert!(client.remote_snapshot(&checkout).is_err());
        assert!(client.verify_remote_binding(&latest).is_err());
    }
    #[test]
    fn default_branch_switch_invalidates_same_commit_binding() {
        let (temp, client, checkout, author) = fixture();
        let first = client.remote_snapshot(&checkout).unwrap();
        git(&author, &["push", "origin", "HEAD:refs/heads/stable"]);
        fs::write(temp.path().join("default-branch"), "stable").unwrap();
        assert!(client.verify_remote_binding(&first).is_err());
        let current = client.remote_snapshot(&checkout).unwrap();
        assert_eq!(current.commit, first.commit);
        assert_eq!(current.default_branch, "stable");
    }
    #[test]
    fn skill_hash_and_upstream_release_or_source_must_all_be_current() {
        let (temp, client, _checkout, author) = fixture();
        let installed = temp.path().join("skill");
        fs::create_dir(&installed).unwrap();
        fs::write(installed.join("SKILL.md"), "Use the tested CLI.").unwrap();
        let runtime = installed.join("runtime");
        fs::write(&runtime, b"runtime-v1").unwrap();
        let mut manifest = SkillsManifest {
            schema_version: 1,
            skills: vec![SkillSnapshot {
                name: "workflow-cli".into(),
                repository: "coolplayagent/freshness-fixture".into(),
                installed_path: installed.clone(),
                runtime_path: runtime.clone(),
                tree_sha256: skill_tree_digest(&installed).unwrap(),
                runtime_sha256: hex(&Sha256::digest(b"runtime-v1")),
                release_tag: Some("v1".into()),
                release_id: Some(42),
                source_commit: None,
                source_qualitygate: None,
                checked_at: Utc::now().to_rfc3339(),
            }],
        };
        let path = temp.path().join("skills.json");
        storage::write(&path, &manifest).unwrap();
        client.verify_skills(&path).unwrap();
        verify_installed_bytes(&manifest).unwrap();
        fs::write(installed.join("SKILL.md"), "modified instructions").unwrap();
        assert!(client.verify_skills(&path).is_err());
        assert!(
            verify_installed_bytes(&manifest).is_err(),
            "a mid-attempt installation must invalidate old observations without a network call"
        );
        fs::write(installed.join("SKILL.md"), "Use the tested CLI.").unwrap();
        fs::write(
            temp.path().join("release.json"),
            r#"{"id":43,"tag_name":"v2","draft":false,"prerelease":false}"#,
        )
        .unwrap();
        assert!(client.verify_skills(&path).is_err());
        manifest.skills[0].release_tag = None;
        manifest.skills[0].release_id = None;
        manifest.skills[0].source_commit = Some(git(&author, &["rev-parse", "HEAD"]));
        storage::write(&path, &manifest).unwrap();
        assert!(
            client.verify_skills(&path).is_err(),
            "an unverified source fallback cannot ignore an existing release"
        );
        let proof_path = temp.path().join("source-gate.json");
        let mut proof = serde_json::json!({
            "profile":"full","scope":"delivery",
            "gate":{"complete":true,"decision":"pass"},
            "plan":{"pending_delivery_checks":[]},
            "selection":{"empty_delivery":false},
            "snapshot":{"mode":"diff","head":manifest.skills[0].source_commit}
        });
        storage::write(&proof_path, &proof).unwrap();
        manifest.skills[0].source_qualitygate = Some(SourceQualitygate {
            path: proof_path.clone(),
            sha256: storage::digest(&fs::read(&proof_path).unwrap()),
        });
        storage::write(&path, &manifest).unwrap();
        client.verify_skills(&path).unwrap();
        let retained_proof = fs::read(&proof_path).unwrap();
        fs::OpenOptions::new()
            .write(true)
            .open(&proof_path)
            .unwrap()
            .set_len(16 * 1024 * 1024 + 1)
            .unwrap();
        assert!(
            verify_source_qualitygate(&manifest.skills[0])
                .unwrap_err()
                .to_string()
                .contains("16 MiB"),
            "oversized reports must be rejected before parsing"
        );
        fs::write(&proof_path, &retained_proof).unwrap();
        let replacement = temp.path().join("replacement-source-gate.json");
        fs::rename(&proof_path, &replacement).unwrap();
        std::os::unix::fs::symlink(&replacement, &proof_path).unwrap();
        assert!(verify_source_qualitygate(&manifest.skills[0]).is_err());
        fs::remove_file(&proof_path).unwrap();
        fs::rename(&replacement, &proof_path).unwrap();
        verify_source_qualitygate(&manifest.skills[0]).unwrap();
        proof["snapshot"]["head"] = "0".repeat(40).into();
        storage::write(&proof_path, &proof).unwrap();
        assert!(
            client.verify_skills(&path).is_err(),
            "changed proof must fail"
        );
        manifest.skills[0]
            .source_qualitygate
            .as_mut()
            .unwrap()
            .sha256 = storage::digest(&fs::read(&proof_path).unwrap());
        storage::write(&path, &manifest).unwrap();
        assert!(
            client.verify_skills(&path).is_err(),
            "a passing gate on another source is insufficient"
        );
        manifest.skills[0].source_qualitygate = None;
        storage::write(&path, &manifest).unwrap();
        fs::write(temp.path().join("no-release"), "").unwrap();
        client.verify_skills(&path).unwrap();
        fs::write(author.join("tracked.txt"), "new source skill version").unwrap();
        git(&author, &["commit", "-am", "source advances"]);
        git(&author, &["push", "origin", "main"]);
        assert!(client.verify_skills(&path).is_err());
    }
    #[test]
    fn tree_digest_matches_public_contract_and_rejects_symlinks() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("b.txt"), b"B").unwrap();
        fs::write(temp.path().join("a.txt"), b"A").unwrap();
        fs::create_dir(temp.path().join("folder")).unwrap();
        fs::write(temp.path().join("folder/a.txt"), b"D").unwrap();
        fs::write(temp.path().join("folder.txt"), b"C").unwrap();
        let mut expected = Sha256::new();
        for (path, content) in [
            ("a.txt", b"A"),
            ("b.txt", b"B"),
            ("folder.txt", b"C"),
            ("folder/a.txt", b"D"),
        ] {
            expected.update(path.as_bytes());
            expected.update([0]);
            expected.update(Sha256::digest(content));
            expected.update(b"\n");
        }
        assert_eq!(
            skill_tree_digest(temp.path()).unwrap(),
            hex(&expected.finalize())
        );
        std::os::unix::fs::symlink(temp.path().join("a.txt"), temp.path().join("alias")).unwrap();
        assert!(skill_tree_digest(temp.path()).is_err());
    }
    #[test]
    fn rejects_untrusted_origins_and_compares_identity_without_check_time() {
        assert!(github_origin("https://github.com/other/repo.git").is_err());
        assert!(github_origin("https://token@github.com/coolplayagent/repo.git").is_err());
        assert_eq!(
            github_origin("git@github.com:stevetdp/superpod.git").unwrap(),
            "stevetdp/superpod"
        );
        let first = RemoteSnapshot {
            repository: "coolplayagent/repo".into(),
            default_branch: "main".into(),
            commit: "a".repeat(40),
            checked_at: Utc::now().to_rfc3339(),
        };
        let mut later = first.clone();
        later.checked_at = "2026-10-09T00:00:00Z".into();
        assert!(first.same_binding(&later));
        assert_ne!(first, later);
    }
}
