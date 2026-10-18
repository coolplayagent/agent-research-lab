//! Reviewed report publication with an explicitly configured repository layout and index adapter.
use anyhow::{Context, Result, ensure};
use delivery::{
    PullRequestReceipt, PullRequestRequest, capture_text, ensure_pull_request, file_sha256,
    relative_path, validate_commit, validate_digest, validate_identifier,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EvidenceBindings {
    pub experiment_id: String,
    pub source_repository: String,
    pub source_commit: String,
    pub prompt_sha256: String,
    pub policy_sha256: String,
    #[serde(rename = "superpod_commit", alias = "knowledge_revision")]
    pub knowledge_revision: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SourceReference {
    pub id: String,
    pub title: String,
    /// Public HTTPS URL, or a source archive path relative to external knowledge repository's root.
    pub resource: String,
    pub sha256: Option<String>,
    pub retrieved_at: String,
    pub access_status: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingKind {
    Measured,
    ExternalClaim,
    Inference,
    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Finding {
    pub kind: FindingKind,
    pub statement: String,
    pub source_ids: Vec<String>,
    pub method: String,
    pub limitations: String,
    /// A digest of an explicitly reviewed public evidence artifact, never a private run path.
    pub public_evidence_sha256: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OutcomeReport {
    pub title: String,
    pub summary: String,
    pub bindings: EvidenceBindings,
    pub sources: Vec<SourceReference>,
    pub findings: Vec<Finding>,
    pub dissenting_views: Vec<String>,
    /// Required attestation by the publishing caller after reviewing the rendered text.
    pub publication_reviewed: bool,
}

fn public_text(text: &str) -> Result<()> {
    let lower = text.to_ascii_lowercase();
    ensure!(
        !text.contains('\0')
            && !lower.contains("-----begin private key-----")
            && !lower.contains("-----begin openssh private key-----")
            && !lower.contains("ghp_")
            && !lower.contains("github_pat_")
            && !lower.contains("sk-proj-")
            && !lower.contains("authorization: bearer")
            && !lower.contains(".codex/auth.json"),
        "publication contains credential-like or private configuration content"
    );
    // This is a tripwire, not a claim that arbitrary prose can be automatically declassified.
    ensure!(
        !lower.contains("holdout_answer")
            && !lower.contains("private_holdout")
            && !lower.contains("raw_session_log"),
        "private evaluation or raw session content cannot be published"
    );
    Ok(())
}

pub fn validate_report(report: &OutcomeReport, knowledge_root: &Path) -> Result<()> {
    ensure!(
        report.publication_reviewed,
        "publication content review is required"
    );
    validate_identifier(&report.bindings.experiment_id)?;
    ensure!(
        report.bindings.experiment_id != "index",
        "experiment ID is reserved for navigation"
    );
    validate_commit(&report.bindings.source_commit)?;
    validate_commit(&report.bindings.knowledge_revision)?;
    validate_digest(&report.bindings.prompt_sha256)?;
    validate_digest(&report.bindings.policy_sha256)?;
    ensure!(
        report
            .bindings
            .source_repository
            .starts_with("https://github.com/")
            && !report
                .bindings
                .source_repository
                .contains(['?', '#', '@', '\n']),
        "source repository must be a public GitHub URL"
    );
    ensure!(
        !report.title.trim().is_empty()
            && !report.summary.trim().is_empty()
            && !report.findings.is_empty(),
        "report needs title, summary and findings"
    );
    public_text(&serde_json::to_string(report)?)?;
    let mut ids = BTreeSet::new();
    for source in &report.sources {
        validate_identifier(&source.id)?;
        if let Some(digest) = &source.sha256 {
            validate_digest(digest)?;
        }
        ensure!(ids.insert(source.id.clone()), "duplicate source ID");
        ensure!(!source.title.trim().is_empty(), "source title is required");
        chrono::DateTime::parse_from_rfc3339(&source.retrieved_at)
            .context("source retrieval time must be RFC3339")?;
        ensure!(
            [
                "downloaded-verified",
                "online-only",
                "access-gated",
                "not-found",
                "candidate"
            ]
            .contains(&source.access_status.as_str()),
            "unknown external knowledge repository source catalog access status"
        );
        if source.resource.starts_with("https://") {
            ensure!(
                source.access_status != "downloaded-verified",
                "downloaded-verified sources must identify their local archive"
            );
            let authority = source
                .resource
                .trim_start_matches("https://")
                .split('/')
                .next()
                .unwrap_or("");
            ensure!(
                !authority.is_empty()
                    && !source.resource.contains(['@', '?', '#', '\n', '\r', ' '])
                    && authority != "localhost"
                    && !authority.starts_with("127."),
                "source URL must be a normalized public URL without credentials or query tokens"
            );
        } else {
            let path = Path::new(&source.resource);
            relative_path(path)?;
            ensure!(
                path.starts_with("sources"),
                "local sources must use external knowledge repository's sources archive"
            );
            let resolved =
                fs::canonicalize(knowledge_root.join(path)).context("source archive is missing")?;
            ensure!(
                resolved.starts_with(fs::canonicalize(knowledge_root.join("sources"))?),
                "source escapes archive"
            );
            ensure!(
                Some(file_sha256(&resolved)?) == source.sha256,
                "source archive checksum mismatch"
            );
        }
    }
    let mut used = BTreeSet::new();
    for finding in &report.findings {
        ensure!(
            !finding.statement.trim().is_empty() && !finding.limitations.trim().is_empty(),
            "findings require a statement and limitations"
        );
        for id in &finding.source_ids {
            ensure!(ids.contains(id), "finding cites unknown source: {id}");
            used.insert(id.clone());
        }
        match finding.kind {
            FindingKind::Measured => {
                ensure!(
                    !finding.method.trim().is_empty(),
                    "measured claims require a method"
                );
                validate_digest(
                    finding
                        .public_evidence_sha256
                        .as_deref()
                        .context("measured claims require public evidence digest")?,
                )?;
            }
            FindingKind::ExternalClaim | FindingKind::Inference => ensure!(
                !finding.source_ids.is_empty(),
                "external claims and inferences require citations"
            ),
            FindingKind::Unknown => {}
        }
    }
    ensure!(
        ids == used,
        "every listed source must be cited by a finding"
    );
    Ok(())
}

fn yaml_quote(value: &str) -> String {
    serde_json::to_string(value).expect("string serialization cannot fail")
}

pub fn render_report(report: &OutcomeReport) -> String {
    let mut text = format!(
        "---\ntype: Research Experiment\ntitle: {}\nstatus: emerging\ntags: [ai-sdlc, long-horizon-agents, agent-research]\nexperiment_id: {}\nsource_repository: {}\nsource_commit: {}\nprompt_sha256: {}\npolicy_sha256: {}\nsuperpod_baseline: {}\nsources:\n",
        yaml_quote(&report.title),
        yaml_quote(&report.bindings.experiment_id),
        yaml_quote(&report.bindings.source_repository),
        yaml_quote(&report.bindings.source_commit),
        yaml_quote(&report.bindings.prompt_sha256),
        yaml_quote(&report.bindings.policy_sha256),
        yaml_quote(&report.bindings.knowledge_revision)
    );
    if report.sources.is_empty() {
        text.push_str("  []\n");
    }
    for source in &report.sources {
        let resource = if source.resource.starts_with("https://") {
            source.resource.clone()
        } else {
            format!("../../../{}", source.resource)
        };
        text.push_str(&format!("  - id: {}\n    resource: {}\n    title: {}\n    retrieved_at: {}\n    access_status: {}\n", yaml_quote(&source.id), yaml_quote(&resource), yaml_quote(&source.title), yaml_quote(&source.retrieved_at), yaml_quote(&source.access_status)));
        if let Some(digest) = &source.sha256 {
            text.push_str(&format!("    sha256: {}\n", yaml_quote(digest)));
        }
    }
    text.push_str(&format!(
        "---\n\n# {}\n\n{}\n\n## Findings\n",
        report.title, report.summary
    ));
    for finding in &report.findings {
        let kind = match finding.kind {
            FindingKind::Measured => "Measured",
            FindingKind::ExternalClaim => "External claim",
            FindingKind::Inference => "Inference",
            FindingKind::Unknown => "Unknown",
        };
        let citations = finding
            .source_ids
            .iter()
            .map(|id| format!("[^{id}]"))
            .collect::<Vec<_>>()
            .join(" ");
        text.push_str(&format!(
            "\n### {kind}\n\n{} {}\n\nMethod: {}\n\nLimitations: {}\n",
            finding.statement, citations, finding.method, finding.limitations
        ));
        if let Some(digest) = &finding.public_evidence_sha256 {
            text.push_str(&format!("\nPublic evidence SHA-256: `{digest}`\n"));
        }
    }
    if !report.dissenting_views.is_empty() {
        text.push_str("\n## Dissent and unresolved questions\n");
        for view in &report.dissenting_views {
            text.push_str(&format!("\n- {view}\n"));
        }
    }
    text.push_str(&format!("\n## Reproducibility\n\nExperiment `{}`; [source commit]({}/commit/{}); prompt SHA-256 `{}`; policy SHA-256 `{}`; external knowledge repository baseline `{}`.\n", report.bindings.experiment_id, report.bindings.source_repository.trim_end_matches('/'), report.bindings.source_commit, report.bindings.prompt_sha256, report.bindings.policy_sha256, report.bindings.knowledge_revision));
    for source in &report.sources {
        let resource = if source.resource.starts_with("https://") {
            source.resource.clone()
        } else {
            format!("../../../{}", source.resource)
        };
        let checksum = source
            .sha256
            .as_ref()
            .map(|digest| format!("SHA-256 `{digest}`."))
            .unwrap_or_else(|| "Content checksum unavailable.".into());
        text.push_str(&format!(
            "\n[^{}]: [{}](<{}>). Retrieved {}; {}. {}\n",
            source.id,
            source.title.replace(']', "\\]"),
            resource,
            source.retrieved_at,
            source.access_status,
            checksum
        ));
    }
    text
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationLayout {
    pub report_directory: PathBuf,
    pub navigation_files: Vec<PathBuf>,
    pub generated_roots: Vec<PathBuf>,
    pub topic: String,
    pub repository: String,
    pub expected_origin: String,
}
impl PublicationLayout {
    fn validate(&self) -> Result<()> {
        relative_path(&self.report_directory)?;
        validate_identifier(&self.topic)?;
        ensure!(
            self.repository.split('/').count() == 2
                && self
                    .repository
                    .split('/')
                    .all(|v| validate_identifier(v).is_ok()),
            "invalid publication repository"
        );
        ensure!(
            !self.expected_origin.is_empty() && self.expected_origin.len() <= 2048,
            "explicit publication origin required"
        );
        ensure!(
            self.navigation_files.len() <= 32 && self.generated_roots.len() <= 16,
            "publication layout exceeds bound"
        );
        for path in self.navigation_files.iter().chain(&self.generated_roots) {
            relative_path(path)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PrepareReportRequest {
    pub layout: PublicationLayout,
    #[serde(rename = "superpod_root", alias = "knowledge_root")]
    pub knowledge_root: PathBuf,
    pub worktree: PathBuf,
    pub relay_knowledge_binary: PathBuf,
    pub report: OutcomeReport,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PreparedReport {
    pub layout: PublicationLayout,
    pub worktree: PathBuf,
    pub branch: String,
    pub baseline: String,
    pub report_path: PathBuf,
    pub report_sha256: String,
    pub experiment_id: String,
    pub relay_knowledge_binary: PathBuf,
    /// Every reviewed contribution path, including generated navigation/maps and deletions.
    pub publication_files: BTreeMap<PathBuf, Option<String>>,
}

fn git(cwd: &Path, args: &[&str]) -> Result<String> {
    capture_text(
        "git",
        &args.iter().map(|v| (*v).into()).collect::<Vec<_>>(),
        cwd,
    )
}

fn relay(binary: &Path, cwd: &Path, args: &[&str]) -> Result<Value> {
    let result = capture_text(
        binary.to_str().context("non-UTF-8 relay-knowledge path")?,
        &args.iter().map(|v| (*v).into()).collect::<Vec<_>>(),
        cwd,
    )?;
    let value: Value =
        serde_json::from_str(&result).context("relay-knowledge did not return JSON")?;
    ensure!(
        value.get("error_kind").is_none(),
        "relay-knowledge returned an error"
    );
    if args.starts_with(&["map", "validate"]) {
        ensure!(
            value["results"]
                .as_array()
                .is_some_and(|results| !results.is_empty()
                    && results.iter().all(|result| result["valid"] == true)),
            "map validation is incomplete or failed"
        );
    }
    Ok(value)
}

fn add_index_line(path: &Path, heading: &str, line: &str) -> Result<()> {
    let mut text = if path.exists() {
        fs::read_to_string(path)?
    } else {
        format!("{heading}\n")
    };
    if !text.lines().any(|existing| existing == line) {
        text.push_str(&format!("\n{line}\n"));
        fs::write(path, text)?;
    }
    Ok(())
}

/// Prepare a reviewable contribution on an isolated branch; never mutate external knowledge repository's current checkout.
pub fn prepare_report(request: &PrepareReportRequest) -> Result<PreparedReport> {
    ensure!(
        request.knowledge_root.is_absolute() && request.worktree.is_absolute(),
        "external knowledge repository and worktree paths must be absolute"
    );
    request.layout.validate()?;
    ensure!(
        git(&request.knowledge_root, &["remote", "get-url", "origin"])?
            == request.layout.expected_origin,
        "publication origin mismatch"
    );
    validate_report(&request.report, &request.knowledge_root)?;
    let baseline = &request.report.bindings.knowledge_revision;
    let resolved = git(
        &request.knowledge_root,
        &["rev-parse", &format!("{baseline}^{{commit}}")],
    )?;
    ensure!(
        &resolved == baseline,
        "external knowledge repository baseline does not resolve exactly"
    );
    let branch = format!("outcomes/{}", request.report.bindings.experiment_id);
    if request.worktree.exists() {
        let source_common = fs::canonicalize(request.knowledge_root.join(git(
            &request.knowledge_root,
            &["rev-parse", "--git-common-dir"],
        )?))?;
        let worktree_common = fs::canonicalize(
            request
                .worktree
                .join(git(&request.worktree, &["rev-parse", "--git-common-dir"])?),
        )?;
        ensure!(
            source_common == worktree_common,
            "existing worktree belongs to another repository"
        );
        ensure!(
            git(&request.worktree, &["branch", "--show-current"])? == branch,
            "worktree belongs to a different branch"
        );
        ensure!(
            git(&request.worktree, &["rev-parse", "HEAD"])? == *baseline,
            "worktree advanced; reconcile previous publication first"
        );
    } else {
        let worktree = request
            .worktree
            .to_str()
            .context("non-UTF-8 worktree path")?;
        git(
            &request.knowledge_root,
            &["worktree", "add", "-b", &branch, worktree, baseline],
        )?;
    }
    validate_report(&request.report, &request.worktree)?;
    relay(
        &request.relay_knowledge_binary,
        &request.worktree,
        &["map", "validate", "--format", "json"],
    )?;
    let folder = request.worktree.join(&request.layout.report_directory);
    fs::create_dir_all(&folder)?;
    let relative = request
        .layout
        .report_directory
        .join(format!("{}.md", request.report.bindings.experiment_id));
    let report_path = request.worktree.join(&relative);
    let rendered = render_report(&request.report);
    if report_path.exists() {
        ensure!(
            fs::read_to_string(&report_path)? == rendered,
            "existing report differs; use a new experiment ID or review an explicit revision"
        );
    } else {
        fs::write(&report_path, rendered)?;
    }
    add_index_line(
        &folder.join("index.md"),
        "# Published outcomes",
        &format!(
            "- [{}]({}.md)",
            request.report.title.replace(['\n', '\r'], " "),
            request.report.bindings.experiment_id
        ),
    )?;
    // Routing artifacts are generated exclusively by the published CLI.
    let source_id = format!("outcome-{}", request.report.bindings.experiment_id);
    let shown = relay(
        &request.relay_knowledge_binary,
        &request.worktree,
        &["map", "show", "--type", "knowledge", "--format", "json"],
    )?;
    let sources = shown["map"]["sources"]
        .as_array()
        .context("map source inventory missing")?;
    if let Some(existing) = sources.iter().find(|source| source["id"] == source_id) {
        ensure!(
            existing["uri"].as_str() == relative.to_str()
                && existing["topic"] == request.layout.topic,
            "existing knowledge source ID has another route"
        );
    } else {
        relay(
            &request.relay_knowledge_binary,
            &request.worktree,
            &[
                "map",
                "source",
                "add",
                "--type",
                "knowledge",
                "--id",
                &source_id,
                "--topic",
                &request.layout.topic,
                "--kind",
                "file",
                "--uri",
                relative.to_str().context("invalid report path")?,
                "--description",
                &request.report.title,
                "--format",
                "json",
            ],
        )?;
    }
    relay(
        &request.relay_knowledge_binary,
        &request.worktree,
        &["map", "validate", "--format", "json"],
    )?;
    let mut prepared = PreparedReport {
        layout: request.layout.clone(),
        worktree: fs::canonicalize(&request.worktree)?,
        branch,
        baseline: baseline.clone(),
        report_path: relative,
        report_sha256: file_sha256(&report_path)?,
        experiment_id: request.report.bindings.experiment_id.clone(),
        relay_knowledge_binary: request.relay_knowledge_binary.clone(),
        publication_files: BTreeMap::new(),
    };
    prepared.publication_files = publication_snapshot(&prepared, None)?;
    Ok(prepared)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PublishReportRequest {
    pub prepared: PreparedReport,
    pub base_branch: String,
    pub title: String,
    pub body: String,
}

fn allowed_publication_path(path: &Path, report: &PreparedReport) -> bool {
    path == report.report_path
        || path == report.layout.report_directory.join("index.md")
        || report
            .layout
            .navigation_files
            .iter()
            .any(|allowed| path == allowed)
        || report
            .layout
            .generated_roots
            .iter()
            .any(|root| path.starts_with(root))
}

fn nul_paths(output: std::process::Output) -> Result<Vec<PathBuf>> {
    ensure!(output.status.success(), "cannot inspect publication diff");
    Ok(String::from_utf8(output.stdout)?
        .split('\0')
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .collect())
}

fn publication_snapshot(
    prepared: &PreparedReport,
    commit: Option<&str>,
) -> Result<BTreeMap<PathBuf, Option<String>>> {
    let mut args = vec![
        "diff".into(),
        "--name-only".into(),
        "-z".into(),
        prepared.baseline.clone(),
    ];
    if let Some(commit) = commit {
        args.push(commit.into());
    }
    args.push("--".into());
    let mut paths: BTreeSet<PathBuf> = nul_paths(process::capture(
        "git",
        &args,
        &prepared.worktree,
        std::time::Duration::from_secs(30),
    )?)?
    .into_iter()
    .collect();
    if commit.is_none() {
        for args in [
            vec!["ls-files", "--others", "--exclude-standard", "-z"],
            vec!["diff", "--cached", "--name-only", "-z", "--"],
        ] {
            paths.extend(nul_paths(process::capture(
                "git",
                &args.into_iter().map(String::from).collect::<Vec<_>>(),
                &prepared.worktree,
                std::time::Duration::from_secs(30),
            )?)?);
        }
    }
    let mut result = BTreeMap::new();
    for path in paths {
        relative_path(&path)?;
        ensure!(
            allowed_publication_path(&path, prepared),
            "unexpected publication file: {}",
            path.display()
        );
        let digest = if let Some(commit) = commit {
            let entry = git(
                &prepared.worktree,
                &[
                    "ls-tree",
                    commit,
                    "--",
                    path.to_str().context("non-UTF-8 publication path")?,
                ],
            )?;
            if entry.is_empty() {
                None
            } else {
                ensure!(
                    entry.starts_with("100644 blob ") || entry.starts_with("100755 blob "),
                    "publication tree entry must be a regular file"
                );
                let output = process::capture(
                    "git",
                    &["show".into(), format!("{commit}:{}", path.display())],
                    &prepared.worktree,
                    std::time::Duration::from_secs(30),
                )?;
                ensure!(
                    output.status.success(),
                    "cannot read committed publication file"
                );
                Some(format!("{:x}", Sha256::digest(&output.stdout)))
            }
        } else {
            let file = prepared.worktree.join(&path);
            match fs::symlink_metadata(&file) {
                Ok(metadata) => {
                    ensure!(
                        metadata.file_type().is_file() && fs::canonicalize(&file)? == file,
                        "publication path must be a regular file without symlink aliases"
                    );
                    Some(file_sha256(&file)?)
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => return Err(error.into()),
            }
        };
        result.insert(path, digest);
    }
    Ok(result)
}

fn validate_prepared_publication(request: &PublishReportRequest) -> Result<()> {
    let prepared = &request.prepared;
    prepared.layout.validate()?;
    validate_identifier(&prepared.experiment_id)?;
    validate_commit(&prepared.baseline)?;
    validate_digest(&prepared.report_sha256)?;
    ensure!(prepared.experiment_id != "index", "reserved experiment ID");
    let expected_branch = format!("outcomes/{}", prepared.experiment_id);
    ensure!(
        prepared.branch == expected_branch && prepared.branch != request.base_branch,
        "publication must use its dedicated outcome branch, distinct from the base"
    );
    git(
        &prepared.worktree,
        &["check-ref-format", "--branch", &request.base_branch],
    )?;
    ensure!(
        git(&prepared.worktree, &["branch", "--show-current"])? == expected_branch,
        "publication branch changed"
    );
    ensure!(
        prepared.report_path
            == prepared
                .layout
                .report_directory
                .join(format!("{}.md", prepared.experiment_id)),
        "report path does not match the experiment"
    );
    git(
        &prepared.worktree,
        &["merge-base", "--is-ancestor", &prepared.baseline, "HEAD"],
    )?;
    ensure!(
        prepared.publication_files.get(&prepared.report_path)
            == Some(&Some(prepared.report_sha256.clone())),
        "review manifest does not bind the report"
    );
    ensure!(
        publication_snapshot(prepared, None)? == prepared.publication_files,
        "publication snapshot changed after review"
    );
    Ok(())
}

/// Publish only reviewed files and open a pull request in the configured repository.
pub fn publish_report(request: &PublishReportRequest) -> Result<PullRequestReceipt> {
    let prepared = &request.prepared;
    validate_prepared_publication(request)?;
    let remote = git(&prepared.worktree, &["remote", "get-url", "origin"])?;
    ensure!(
        remote == prepared.layout.expected_origin,
        "publication origin changed after review"
    );
    relay(
        &prepared.relay_knowledge_binary,
        &prepared.worktree,
        &["map", "validate", "--format", "json"],
    )?;
    let output = process::capture(
        "git",
        &[
            "status".into(),
            "--porcelain=v1".into(),
            "-z".into(),
            "--untracked-files=all".into(),
        ],
        &prepared.worktree,
        std::time::Duration::from_secs(30),
    )?;
    ensure!(
        output.status.success(),
        "cannot read publication worktree status"
    );
    let status = String::from_utf8(output.stdout)?;
    let mut changed = Vec::new();
    for record in status.split('\0').filter(|s| !s.is_empty()) {
        ensure!(
            record.len() >= 4 && !record[..2].contains(['R', 'C']),
            "renames require explicit publication review"
        );
        let path = PathBuf::from(&record[3..]);
        ensure!(
            allowed_publication_path(&path, prepared),
            "unexpected publication file: {}",
            path.display()
        );
        changed.push(path);
    }
    if !changed.is_empty() {
        let mut args = vec!["add".into(), "--".into()];
        args.extend(changed.iter().map(|p| p.to_string_lossy().into_owned()));
        capture_text("git", &args, &prepared.worktree)?;
        git(&prepared.worktree, &["commit", "-m", &request.title])?;
    }
    let head = git(&prepared.worktree, &["rev-parse", "HEAD"])?;
    ensure!(
        head != prepared.baseline,
        "publication has no committed contribution"
    );
    ensure!(
        publication_snapshot(prepared, Some(&head))? == prepared.publication_files,
        "committed publication differs from reviewed snapshot"
    );
    git(
        &prepared.worktree,
        &[
            "push",
            "origin",
            &format!("{head}:refs/heads/{}", prepared.branch),
        ],
    )?;
    ensure_pull_request(&PullRequestRequest {
        repository: prepared.layout.repository.clone(),
        working_directory: prepared.worktree.clone(),
        branch: prepared.branch.clone(),
        base: request.base_branch.clone(),
        expected_head: head,
        title: request.title.clone(),
        body: request.body.clone(),
    })
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IndexRefreshRequest {
    pub relay_knowledge_binary: PathBuf,
    #[serde(rename = "superpod_root", alias = "knowledge_root")]
    pub knowledge_root: PathBuf,
    pub repository_alias: String,
    pub expected_commit: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IndexRefreshStatus {
    pub expected_commit: String,
    pub ready: bool,
    pub status: Value,
}

/// Submit at most one update. Queued/running tasks are returned for the workflow to poll later.
pub fn refresh_index(request: &IndexRefreshRequest) -> Result<IndexRefreshStatus> {
    validate_commit(&request.expected_commit)?;
    validate_identifier(&request.repository_alias)?;
    let mut status = relay(
        &request.relay_knowledge_binary,
        &request.knowledge_root,
        &[
            "repo",
            "status",
            &request.repository_alias,
            "--format",
            "json",
        ],
    )?;
    fn snapshot(value: &Value) -> &Value {
        value
            .get("status")
            .filter(|v| v.is_object())
            .unwrap_or(value)
    }
    fn ready(value: &Value, expected: &str) -> bool {
        let value = snapshot(value);
        let integrity = &value["content_integrity"];
        // The installed 1.1.18 contract uses state="complete". Retain explicit
        // Boolean compatibility without treating a missing integrity report as proof.
        let complete = match integrity.get("state") {
            Some(state) => {
                contracts::ContentIntegrityState::from_value(state)
                    == Some(contracts::ContentIntegrityState::Complete)
            }
            None => integrity.get("complete") == Some(&Value::Bool(true)),
        };
        value["last_indexed_commit"].as_str() == Some(expected)
            && value["stale"] == false
            && complete
    }
    let snapshot_status = snapshot(&status);
    let active = status.get("active_task").is_some_and(|v| !v.is_null())
        || snapshot_status
            .get("active_task")
            .is_some_and(|v| !v.is_null());
    let maintenance_pending = status["maintenance_pending"] == true
        || status["retention"]["maintenance_pending"] == true
        || snapshot_status["maintenance_pending"] == true
        || snapshot_status["retention"]["maintenance_pending"] == true;
    if !ready(&status, &request.expected_commit) && !active && !maintenance_pending {
        relay(
            &request.relay_knowledge_binary,
            &request.knowledge_root,
            &[
                "repo",
                "update",
                &request.repository_alias,
                "--head",
                &request.expected_commit,
                "--format",
                "json",
            ],
        )?;
        status = relay(
            &request.relay_knowledge_binary,
            &request.knowledge_root,
            &[
                "repo",
                "status",
                &request.repository_alias,
                "--format",
                "json",
            ],
        )?;
    }
    Ok(IndexRefreshStatus {
        expected_commit: request.expected_commit.clone(),
        ready: ready(&status, &request.expected_commit),
        status,
    })
}

/// Stable report payload identity for callers that keep a publication outbox.
pub fn report_digest(report: &OutcomeReport) -> String {
    format!("{:x}", Sha256::digest(render_report(report).as_bytes()))
}

// Compatibility type name for earlier source consumers.
pub use OutcomeReport as ResearchReport;

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
