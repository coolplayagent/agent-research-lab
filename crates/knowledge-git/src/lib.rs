//! Export accepted outcome records into a prepared external Git branch with compare-and-swap.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use service_api::OutcomeServiceRequest;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::Duration,
};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GitTarget {
    pub path: PathBuf,
    pub reference: String,
}

use collaboration_model::{KnowledgeWrite, KnowledgeWriteResult, Outcome};
use contracts::KnowledgeWriteState;
use std::{fs, time::Instant};

const PREFIX: &str = "accepted-outcomes";
#[derive(Serialize, Deserialize)]
struct Receipt {
    request: KnowledgeWrite,
    result: KnowledgeWriteResult,
}

struct Git<'a> {
    target: &'a GitTarget,
    run: tempfile::TempDir,
    deadline: Instant,
    prefix: String,
    remote: Option<String>,
    credential_env: Vec<String>,
}
impl Git<'_> {
    fn call(&self, args: &[&str]) -> Result<String> {
        Ok(self.call_raw(args)?.trim().into())
    }
    fn call_raw(&self, args: &[&str]) -> Result<String> {
        let duration = self.deadline.saturating_duration_since(Instant::now());
        ensure!(!duration.is_zero(), "knowledge operation deadline expired");
        let capture = tempfile::tempdir_in(self.run.path())?;
        let mut arguments: Vec<String> = [
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "commit.gpgSign=false",
            "-C",
        ]
        .into_iter()
        .map(str::to_string)
        .collect();
        arguments.push(
            self.target
                .path
                .to_str()
                .context("repository path must be UTF-8")?
                .into(),
        );
        arguments.extend(args.iter().map(|s| s.to_string()));
        let mut env = vec![
            ("PATH".into(), "/usr/bin:/bin".into()),
            ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
            ("GIT_CONFIG_GLOBAL".into(), "/dev/null".into()),
            ("GIT_TERMINAL_PROMPT".into(), "0".into()),
            (
                "GIT_INDEX_FILE".into(),
                self.run.path().join("index").display().to_string(),
            ),
            (
                "GIT_AUTHOR_NAME".into(),
                "OutcomeHub Knowledge Publisher".into(),
            ),
            ("GIT_AUTHOR_EMAIL".into(), "outcome-hub@localhost".into()),
            (
                "GIT_COMMITTER_NAME".into(),
                "OutcomeHub Knowledge Publisher".into(),
            ),
            ("GIT_COMMITTER_EMAIL".into(), "outcome-hub@localhost".into()),
        ];
        for name in &self.credential_env {
            env.push((
                name.clone(),
                std::env::var(name).context("knowledge credential environment is unavailable")?,
            ));
        }
        let mut child = process::Process::spawn_clean(
            "/usr/bin/git",
            &arguments,
            self.run.path(),
            capture.path(),
            duration.min(Duration::from_secs(20)),
            &env,
            None,
        )?;
        loop {
            if let Some(status) = child.poll()? {
                ensure!(status.success(), "knowledge Git operation failed");
                ensure!(
                    fs::metadata(&child.stdout)?.len() <= 2 * 1024 * 1024,
                    "knowledge output exceeds bound"
                );
                return Ok(fs::read_to_string(&child.stdout)?);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    fn head(&self) -> Result<String> {
        self.call(&["check-ref-format", &self.target.reference])?;
        let value = self.call(&[
            "rev-parse",
            "--verify",
            &format!("{}^{{commit}}", self.target.reference),
        ])?;
        hash(&value)?;
        Ok(value)
    }
    fn receipt(&self, write: &KnowledgeWrite) -> Result<Option<KnowledgeWriteResult>> {
        let prefix = &self.prefix;
        let head = self.head()?;
        let path = format!("{prefix}/receipts/{}.json", write.input.id);
        let listing = self.call(&["ls-tree", "--name-only", &head, "--", &path])?;
        if listing.is_empty() {
            return Ok(None);
        }
        let raw = self.call(&["show", &format!("{head}:{path}")])?;
        let recorded: KnowledgeWrite = serde_json::from_str(&raw)?;
        ensure!(
            recorded.request_digest == write.request_digest && recorded.input == write.input,
            "knowledge request ID conflict"
        );
        let revision = self.call(&["log", "-1", "--format=%H", &head, "--", &path])?;
        hash(&revision)?;
        Ok(Some(result(
            write,
            KnowledgeWriteState::Applied,
            format!("{prefix}/{}.json", write.input.outcome_id),
            revision,
            "Accepted outcome committed to the configured import branch",
        )))
    }
    fn publish(&self, write: &KnowledgeWrite, outcome: &Outcome) -> Result<KnowledgeWriteResult> {
        if let Some(receipt) = self.receipt(write)? {
            return Ok(receipt);
        }
        let prefix = &self.prefix;
        let head = self.head()?;
        if head != knowledge_model::baseline(&write.input.expected_revision) {
            return Ok(result(
                write,
                KnowledgeWriteState::Failed,
                String::new(),
                String::new(),
                "Expected knowledge revision changed; no write was attempted",
            ));
        }
        let worktrees = self.call(&["worktree", "list", "--porcelain"])?;
        ensure!(
            !worktrees
                .lines()
                .any(|line| line == format!("branch {}", self.target.reference)),
            "knowledge target branch is checked out; use a prepared import branch"
        );
        self.call(&["read-tree", &head])?;
        let entries = [
            (
                format!("{prefix}/{}.json", outcome.input.id),
                serde_json::to_vec_pretty(outcome)?,
            ),
            (
                format!("{prefix}/receipts/{}.json", write.input.id),
                serde_json::to_vec_pretty(write)?,
            ),
        ];
        for (index, (path, bytes)) in entries.into_iter().enumerate() {
            let file = self.run.path().join(format!("content-{index}"));
            fs::write(&file, bytes)?;
            let blob = self.call(&[
                "hash-object",
                "-w",
                "--no-filters",
                "--",
                file.to_str().context("non-UTF-8 temporary path")?,
            ])?;
            hash(&blob)?;
            self.call(&[
                "update-index",
                "--add",
                "--cacheinfo",
                &format!("100644,{blob},{path}"),
            ])?;
        }
        let tree = self.call(&["write-tree"])?;
        hash(&tree)?;
        let message = self.run.path().join("message");
        fs::write(
            &message,
            format!(
                "Publish accepted outcome {}\n\nRequest: {}\nAcceptance: {}\n",
                outcome.input.id, write.input.id, write.input.acceptance_id
            ),
        )?;
        let commit = self.call(&[
            "commit-tree",
            &tree,
            "-p",
            &head,
            "-F",
            message.to_str().context("non-UTF-8 temporary path")?,
        ])?;
        hash(&commit)?;
        if let Some(remote) = &self.remote {
            self.call(&[
                "push",
                "--porcelain",
                remote,
                &format!("{commit}:{}", self.target.reference),
            ])?;
        }
        self.call(&["update-ref", &self.target.reference, &commit, &head])?;
        Ok(result(
            write,
            KnowledgeWriteState::Applied,
            format!("{prefix}/{}.json", outcome.input.id),
            commit,
            "Accepted outcome committed to the configured import branch",
        ))
    }
}
fn hash(value: &str) -> Result<()> {
    ensure!(
        [40, 64].contains(&value.len())
            && value
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()),
        "full lowercase commit hash required"
    );
    Ok(())
}
fn result(
    write: &KnowledgeWrite,
    state: KnowledgeWriteState,
    reference: String,
    revision: String,
    summary: &str,
) -> KnowledgeWriteResult {
    KnowledgeWriteResult {
        id: write.input.id.clone(),
        request_digest: write.request_digest.clone(),
        state,
        reference,
        revision,
        summary: summary.into(),
    }
}
pub fn dispatch(
    service: &str,
    repositories: &BTreeMap<String, GitTarget>,
    state: &Path,
    request: OutcomeServiceRequest,
) -> Result<Value> {
    dispatch_with_target(service, repositories, state, request, None)
}
fn dispatch_with_target(
    service: &str,
    repositories: &BTreeMap<String, GitTarget>,
    state: &Path,
    request: OutcomeServiceRequest,
    options: Option<&knowledge_model::Target>,
) -> Result<Value> {
    let _lock = storage::lock(&state.join("publication.lock"))?;
    match request {
        OutcomeServiceRequest::Write { write, outcome } => {
            outcome.validate_digest()?;
            collaboration_model::id(&write.input.id)?;
            collaboration_model::id(&write.input.outcome_id)?;
            hash(knowledge_model::baseline(&write.input.expected_revision))?;
            ensure!(
                write.input.service_id == service
                    && write.input.outcome_id == outcome.input.id
                    && write.input.outcome_digest == outcome.digest,
                "knowledge binding mismatch"
            );
            ensure!(
                write.request_digest == storage::digest(&serde_json::to_vec(&write.input)?),
                "knowledge request digest mismatch"
            );
            ensure!(
                (1..=600).contains(&write.input.max_seconds),
                "invalid knowledge deadline"
            );
            let target = repositories
                .get(&write.input.repository)
                .context("knowledge repository alias is not configured")?;
            let receipt_path = state
                .join("receipts")
                .join(format!("{}.json", write.input.id));
            if receipt_path.exists() {
                let receipt: Receipt = storage::read(&receipt_path)?;
                ensure!(
                    receipt.request.request_digest == write.request_digest
                        && receipt.request.input == write.input,
                    "knowledge request ID conflict"
                );
                return Ok(json!(receipt.result));
            }
            let seconds = (write
                .deadline_ms
                .saturating_sub(collaboration_model::now_ms())
                / 1000)
                .min(write.input.max_seconds);
            ensure!(seconds > 0, "knowledge deadline expired");
            let git = Git {
                target,
                run: tempfile::tempdir_in(state)?,
                deadline: Instant::now() + Duration::from_secs(seconds),
                prefix: prefix(options),
                remote: options.and_then(|t| t.remote.clone()),
                credential_env: options
                    .map(|t| t.credential_env.clone())
                    .unwrap_or_default(),
            };
            let unknown = result(
                &write,
                KnowledgeWriteState::Unknown,
                String::new(),
                String::new(),
                "Knowledge write requires receipt reconciliation",
            );
            storage::write(
                &receipt_path,
                &Receipt {
                    request: write.clone(),
                    result: unknown.clone(),
                },
            )?;
            let value = git.publish(&write, &outcome).unwrap_or(unknown);
            storage::write(
                &receipt_path,
                &Receipt {
                    request: write,
                    result: value.clone(),
                },
            )?;
            Ok(json!(value))
        }
        OutcomeServiceRequest::LookupWrite { id, request_digest } => {
            collaboration_model::id(&id)?;
            let receipt_path = state.join("receipts").join(format!("{id}.json"));
            if !receipt_path.exists() {
                return Ok(Value::Null);
            }
            let receipt: Receipt = storage::read(&receipt_path)?;
            ensure!(
                receipt.request.request_digest == request_digest,
                "knowledge request ID conflict"
            );
            if receipt.result.state != KnowledgeWriteState::Unknown {
                return Ok(json!(receipt.result));
            }
            let target = repositories
                .get(&receipt.request.input.repository)
                .context("knowledge repository alias is not configured")?;
            let git = Git {
                target,
                run: tempfile::tempdir_in(state)?,
                deadline: Instant::now() + Duration::from_secs(30),
                prefix: prefix(options),
                remote: options.and_then(|t| t.remote.clone()),
                credential_env: options
                    .map(|t| t.credential_env.clone())
                    .unwrap_or_default(),
            };
            Ok(json!(
                git.receipt(&receipt.request)?.unwrap_or(receipt.result)
            ))
        }
        _ => anyhow::bail!("unsupported knowledge operation"),
    }
}

fn prefix(target: Option<&knowledge_model::Target>) -> String {
    target
        .map(|t| format!("{}/{}", t.content_root.display(), PREFIX))
        .unwrap_or_else(|| PREFIX.into())
}
pub fn dispatch_target(
    service: &str,
    name: &str,
    target: &knowledge_model::Target,
    state: &Path,
    request: OutcomeServiceRequest,
) -> Result<Value> {
    let repositories = BTreeMap::from([(
        name.into(),
        GitTarget {
            path: target.path.clone(),
            reference: target
                .reference
                .clone()
                .context("knowledge_branch_required")?,
        },
    )]);
    dispatch_with_target(service, &repositories, state, request, Some(target))
}
pub fn inspect(
    target: &knowledge_model::Target,
    state: &Path,
    path: Option<&Path>,
    revision: Option<&str>,
) -> Result<(String, Option<String>)> {
    let legacy = GitTarget {
        path: target.path.clone(),
        reference: target
            .reference
            .clone()
            .context("knowledge_branch_required")?,
    };
    let git = Git {
        target: &legacy,
        run: tempfile::tempdir_in(state)?,
        deadline: Instant::now() + Duration::from_secs(30),
        prefix: prefix(Some(target)),
        remote: target.remote.clone(),
        credential_env: target.credential_env.clone(),
    };
    let head = git.head()?;
    let revision = revision.unwrap_or(&head);
    hash(revision)?;
    if let Some(path) = path {
        knowledge_model::relative(path)?;
        let full = target
            .content_root
            .join(path)
            .to_str()
            .context("knowledge_invalid_path")?
            .to_string();
        let entry = git.call(&["ls-tree", revision, "--", &full])?;
        if entry.is_empty() {
            return Ok((head, None));
        }
        ensure!(
            entry.starts_with("100644 blob ") || entry.starts_with("100755 blob "),
            "knowledge_regular_blob_required"
        );
        let content = git.call_raw(&["show", &format!("{revision}:{full}")])?;
        Ok((revision.into(), Some(content)))
    } else {
        Ok((head, None))
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
