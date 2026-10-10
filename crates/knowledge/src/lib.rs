//! SuperPOD is the single publication destination. Runtime state and held-out tasks stay local.
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
    pub superpod_commit: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SourceReference {
    pub id: String,
    pub title: String,
    /// Public HTTPS URL, or a source archive path relative to SuperPOD's root.
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
pub struct ResearchReport {
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

pub fn validate_report(report: &ResearchReport, superpod_root: &Path) -> Result<()> {
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
    validate_commit(&report.bindings.superpod_commit)?;
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
            "unknown SuperPOD source catalog access status"
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
                "local sources must use SuperPOD's sources archive"
            );
            let resolved =
                fs::canonicalize(superpod_root.join(path)).context("source archive is missing")?;
            ensure!(
                resolved.starts_with(fs::canonicalize(superpod_root.join("sources"))?),
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

pub fn render_report(report: &ResearchReport) -> String {
    let mut text = format!(
        "---\ntype: Research Experiment\ntitle: {}\nstatus: emerging\ntags: [ai-sdlc, long-horizon-agents, agent-research]\nexperiment_id: {}\nsource_repository: {}\nsource_commit: {}\nprompt_sha256: {}\npolicy_sha256: {}\nsuperpod_baseline: {}\nsources:\n",
        yaml_quote(&report.title),
        yaml_quote(&report.bindings.experiment_id),
        yaml_quote(&report.bindings.source_repository),
        yaml_quote(&report.bindings.source_commit),
        yaml_quote(&report.bindings.prompt_sha256),
        yaml_quote(&report.bindings.policy_sha256),
        yaml_quote(&report.bindings.superpod_commit)
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
    text.push_str(&format!("\n## Reproducibility\n\nExperiment `{}`; [source commit]({}/commit/{}); prompt SHA-256 `{}`; policy SHA-256 `{}`; SuperPOD baseline `{}`.\n", report.bindings.experiment_id, report.bindings.source_repository.trim_end_matches('/'), report.bindings.source_commit, report.bindings.prompt_sha256, report.bindings.policy_sha256, report.bindings.superpod_commit));
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
pub struct PrepareReportRequest {
    pub superpod_root: PathBuf,
    pub worktree: PathBuf,
    pub relay_knowledge_binary: PathBuf,
    pub report: ResearchReport,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PreparedReport {
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

/// Prepare a reviewable contribution on an isolated branch; never mutate SuperPOD's current checkout.
pub fn prepare_report(request: &PrepareReportRequest) -> Result<PreparedReport> {
    ensure!(
        request.superpod_root.is_absolute() && request.worktree.is_absolute(),
        "SuperPOD and worktree paths must be absolute"
    );
    validate_report(&request.report, &request.superpod_root)?;
    let baseline = &request.report.bindings.superpod_commit;
    let resolved = git(
        &request.superpod_root,
        &["rev-parse", &format!("{baseline}^{{commit}}")],
    )?;
    ensure!(
        &resolved == baseline,
        "SuperPOD baseline does not resolve exactly"
    );
    let branch = format!("research/{}", request.report.bindings.experiment_id);
    if request.worktree.exists() {
        let source_common = fs::canonicalize(request.superpod_root.join(git(
            &request.superpod_root,
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
            &request.superpod_root,
            &["worktree", "add", "-b", &branch, worktree, baseline],
        )?;
    }
    validate_report(&request.report, &request.worktree)?;
    relay(
        &request.relay_knowledge_binary,
        &request.worktree,
        &["map", "validate", "--format", "json"],
    )?;
    let folder = request.worktree.join("knowledge/software/ai-sdlc");
    fs::create_dir_all(&folder)?;
    let relative = PathBuf::from(format!(
        "knowledge/software/ai-sdlc/{}.md",
        request.report.bindings.experiment_id
    ));
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
        "# AI-SDLC and long-horizon agent experiments",
        &format!(
            "- [{}]({}.md)",
            request.report.title.replace(['\n', '\r'], " "),
            request.report.bindings.experiment_id
        ),
    )?;
    add_index_line(
        &request.worktree.join("knowledge/software/index.md"),
        "# Software",
        "- [AI-SDLC and long-horizon agent experiments](ai-sdlc/index.md)",
    )?;
    // Routing artifacts are generated exclusively by the published CLI.
    let source_id = format!("agent-research-{}", request.report.bindings.experiment_id);
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
            existing["uri"].as_str() == relative.to_str() && existing["topic"] == "ai-sdlc",
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
                "ai-sdlc",
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
        || path == Path::new("knowledge/software/ai-sdlc/index.md")
        || path == Path::new("knowledge/software/index.md")
        || path == Path::new("knowledge/knowledge-map.yaml")
        || path.starts_with("knowledge/topics")
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
    validate_identifier(&prepared.experiment_id)?;
    validate_commit(&prepared.baseline)?;
    validate_digest(&prepared.report_sha256)?;
    ensure!(prepared.experiment_id != "index", "reserved experiment ID");
    let expected_branch = format!("research/{}", prepared.experiment_id);
    ensure!(
        prepared.branch == expected_branch && prepared.branch != request.base_branch,
        "publication must use its dedicated research branch, distinct from the base"
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
            == Path::new(&format!(
                "knowledge/software/ai-sdlc/{}.md",
                prepared.experiment_id
            )),
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

/// Explicit publication commits only report/navigation/map files and opens a PR in stevetdp/superpod.
pub fn publish_report(request: &PublishReportRequest) -> Result<PullRequestReceipt> {
    let prepared = &request.prepared;
    validate_prepared_publication(request)?;
    let remote = git(&prepared.worktree, &["remote", "get-url", "origin"])?;
    ensure!(
        [
            "https://github.com/stevetdp/superpod.git",
            "https://github.com/stevetdp/superpod",
            "git@github.com:stevetdp/superpod.git"
        ]
        .contains(&remote.as_str()),
        "SuperPOD origin is not stevetdp/superpod"
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
        repository: "stevetdp/superpod".into(),
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
    pub superpod_root: PathBuf,
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
        &request.superpod_root,
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
            &request.superpod_root,
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
            &request.superpod_root,
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
pub fn report_digest(report: &ResearchReport) -> String {
    format!("{:x}", Sha256::digest(render_report(report).as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report() -> ResearchReport {
        ResearchReport {
            title: "A bounded experiment".into(),
            summary: "A hypothesis pending evaluation.".into(),
            bindings: EvidenceBindings {
                experiment_id: "experiment-1".into(),
                source_repository: "https://github.com/coolplayagent/agent-research-lab".into(),
                source_commit: "a".repeat(40),
                prompt_sha256: "b".repeat(64),
                policy_sha256: "c".repeat(64),
                superpod_commit: "d".repeat(40),
            },
            sources: vec![SourceReference {
                id: "reference".into(),
                title: "Primary reference".into(),
                resource: "https://example.org/paper".into(),
                sha256: Some("e".repeat(64)),
                retrieved_at: "2026-10-09T12:00:00+08:00".into(),
                access_status: "online-only".into(),
            }],
            findings: vec![Finding {
                kind: FindingKind::Inference,
                statement: "Recovery may help long tasks.".into(),
                source_ids: vec!["reference".into()],
                method: "Literature comparison".into(),
                limitations: "No experiment has tested this hypothesis.".into(),
                public_evidence_sha256: None,
            }],
            dissenting_views: vec!["Coordination overhead may outweigh the benefit.".into()],
            publication_reviewed: true,
        }
    }

    #[test]
    fn citations_and_publication_boundary_are_validated() {
        let root = tempfile::tempdir().unwrap();
        let mut report = report();
        validate_report(&report, root.path()).unwrap();
        assert!(render_report(&report).contains("[^reference]"));
        report.findings[0].source_ids = vec!["missing".into()];
        assert!(validate_report(&report, root.path()).is_err());
        report.findings[0].source_ids = vec!["reference".into()];
        report.publication_reviewed = false;
        assert!(validate_report(&report, root.path()).is_err());
        report.publication_reviewed = true;
        report.summary = "private_holdout: secret".into();
        assert!(validate_report(&report, root.path()).is_err());
    }

    #[test]
    fn source_archive_checksum_and_confinement_are_enforced() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("sources")).unwrap();
        fs::write(root.path().join("sources/paper.md"), "Public paper").unwrap();
        let mut report = report();
        report.sources[0].resource = "sources/paper.md".into();
        report.sources[0].sha256 =
            Some(file_sha256(&root.path().join("sources/paper.md")).unwrap());
        validate_report(&report, root.path()).unwrap();
        report.sources[0].resource = "sources/../private.md".into();
        assert!(validate_report(&report, root.path()).is_err());
    }

    #[cfg(unix)]
    fn preparation_fixture() -> (tempfile::TempDir, PrepareReportRequest) {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("superpod");
        fs::create_dir(&root).unwrap();
        git(&root, &["init", "-b", "main"]).unwrap();
        git(&root, &["config", "user.name", "Test"]).unwrap();
        git(&root, &["config", "user.email", "test@example.org"]).unwrap();
        fs::create_dir_all(root.join("knowledge/software")).unwrap();
        fs::write(root.join("knowledge/software/index.md"), "# Software\n").unwrap();
        git(&root, &["add", "."]).unwrap();
        git(&root, &["commit", "-m", "baseline"]).unwrap();
        let binary = temp.path().join("relay-knowledge");
        fs::write(
            &binary,
            "#!/bin/sh\nprintf '{\"results\":[{\"valid\":true}],\"map\":{\"sources\":[]}}\\n'\n",
        )
        .unwrap();
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
        let mut report = report();
        report.bindings.superpod_commit = git(&root, &["rev-parse", "HEAD"]).unwrap();
        let request = PrepareReportRequest {
            superpod_root: root.clone(),
            worktree: temp.path().join("contribution"),
            relay_knowledge_binary: binary,
            report,
        };
        (temp, request)
    }

    #[cfg(unix)]
    #[test]
    fn preparation_isolated_from_main_checkout_and_retry_safe() {
        let (_temp, request) = preparation_fixture();
        let root = &request.superpod_root;
        let prepared = prepare_report(&request).unwrap();
        assert!(!root.join(&prepared.report_path).exists());
        assert!(git(root, &["status", "--porcelain"]).unwrap().is_empty());
        assert_eq!(
            prepare_report(&request).unwrap().report_sha256,
            prepared.report_sha256
        );
    }

    #[cfg(unix)]
    fn publication_fixture() -> (tempfile::TempDir, PublishReportRequest) {
        let (temp, request) = preparation_fixture();
        let prepared = prepare_report(&request).unwrap();
        (
            temp,
            PublishReportRequest {
                prepared,
                base_branch: "main".into(),
                title: "Publish test report".into(),
                body: "Test fixture".into(),
            },
        )
    }

    #[cfg(unix)]
    #[test]
    fn publication_rejects_extra_committed_files() {
        let (_temp, request) = publication_fixture();
        validate_prepared_publication(&request).unwrap();
        fs::write(
            request.prepared.worktree.join("unreviewed.txt"),
            "must not publish",
        )
        .unwrap();
        git(&request.prepared.worktree, &["add", "unreviewed.txt"]).unwrap();
        git(
            &request.prepared.worktree,
            &["commit", "-m", "unreviewed file"],
        )
        .unwrap();
        assert!(
            validate_prepared_publication(&request)
                .unwrap_err()
                .to_string()
                .contains("unexpected publication file")
        );
    }

    #[cfg(unix)]
    #[test]
    fn publication_rejects_forged_main_and_same_base_before_effects() {
        let (_temp, mut request) = publication_fixture();
        request.prepared.branch = "main".into();
        assert!(
            publish_report(&request)
                .unwrap_err()
                .to_string()
                .contains("dedicated research branch")
        );
        request.prepared.branch = format!("research/{}", request.prepared.experiment_id);
        request.base_branch = request.prepared.branch.clone();
        assert!(
            publish_report(&request)
                .unwrap_err()
                .to_string()
                .contains("dedicated research branch")
        );
        assert_eq!(
            git(&request.prepared.worktree, &["rev-parse", "HEAD"]).unwrap(),
            request.prepared.baseline
        );
    }

    #[cfg(unix)]
    #[test]
    fn publication_binds_index_map_and_committed_snapshot() {
        let (_temp, mut request) = publication_fixture();
        let index = request
            .prepared
            .worktree
            .join("knowledge/software/index.md");
        let original = fs::read(&index).unwrap();
        fs::write(&index, "changed after review").unwrap();
        assert!(validate_prepared_publication(&request).is_err());
        fs::write(&index, original).unwrap();
        let map = request
            .prepared
            .worktree
            .join("knowledge/knowledge-map.yaml");
        fs::write(&map, "fixture map").unwrap();
        request.prepared.publication_files = publication_snapshot(&request.prepared, None).unwrap();
        fs::write(&map, "map changed after review").unwrap();
        assert!(validate_prepared_publication(&request).is_err());
        fs::write(&map, "fixture map").unwrap();
        validate_prepared_publication(&request).unwrap();
        git(&request.prepared.worktree, &["add", "."]).unwrap();
        git(
            &request.prepared.worktree,
            &["commit", "-m", "reviewed contribution"],
        )
        .unwrap();
        let head = git(&request.prepared.worktree, &["rev-parse", "HEAD"]).unwrap();
        assert_eq!(
            publication_snapshot(&request.prepared, Some(&head)).unwrap(),
            request.prepared.publication_files
        );
        validate_prepared_publication(&request).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn index_refresh_respects_active_work_and_requires_complete_exact_snapshot() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let binary = temp.path().join("relay-knowledge-fixture");
        fs::write(
            &binary,
            r#"#!/bin/sh
set -eu
case "$1 $2" in
  'repo status') cat status.json ;;
  'repo update') printf 'update\n' >> updates.log; cp after.json status.json; printf '{}\n' ;;
  *) exit 9 ;;
esac
"#,
        )
        .unwrap();
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
        let expected = "a".repeat(40);
        let request = IndexRefreshRequest {
            relay_knowledge_binary: binary,
            superpod_root: temp.path().into(),
            repository_alias: "superpod".into(),
            expected_commit: expected.clone(),
        };
        let complete = serde_json::json!({"status":{"last_indexed_commit":expected,"stale":false,"content_integrity":{"state":"complete"}},"retention":{"maintenance_pending":false}});
        let old = serde_json::json!({"status":{"last_indexed_commit":"b".repeat(40),"stale":true,"content_integrity":{"state":"complete"}}});
        let mut cases = Vec::new();
        let mut active = old.clone();
        active["active_task"] = serde_json::json!({"state":"running"});
        cases.push(("top-level active task", active, complete.clone(), false, 0));
        let mut nested = old.clone();
        nested["status"]["active_task"] = serde_json::json!({"state":"queued"});
        cases.push(("legacy nested task", nested, complete.clone(), false, 0));
        let mut maintenance = old.clone();
        maintenance["retention"] = serde_json::json!({"maintenance_pending":true});
        cases.push((
            "maintenance in progress",
            maintenance,
            complete.clone(),
            false,
            0,
        ));
        let mut incomplete = complete.clone();
        incomplete["status"]["content_integrity"]["state"] = serde_json::json!("incomplete");
        cases.push(("incomplete bytes", incomplete.clone(), incomplete, false, 1));
        let missing = serde_json::json!({"status":{"last_indexed_commit":expected,"stale":false}});
        cases.push((
            "missing integrity proof",
            missing.clone(),
            missing,
            false,
            1,
        ));
        let mut conflicting = complete.clone();
        conflicting["status"]["content_integrity"] =
            serde_json::json!({"state":"degraded","complete":true});
        cases.push((
            "contradictory integrity proof",
            conflicting.clone(),
            conflicting,
            false,
            1,
        ));
        cases.push((
            "already complete",
            complete.clone(),
            complete.clone(),
            true,
            0,
        ));
        cases.push(("one bounded update reaches target", old, complete, true, 1));
        for (name, before, after, ready, updates) in cases {
            fs::write(
                temp.path().join("status.json"),
                serde_json::to_vec(&before).unwrap(),
            )
            .unwrap();
            fs::write(
                temp.path().join("after.json"),
                serde_json::to_vec(&after).unwrap(),
            )
            .unwrap();
            fs::write(temp.path().join("updates.log"), "").unwrap();
            let result = refresh_index(&request).unwrap();
            assert_eq!(result.ready, ready, "{name}");
            assert_eq!(
                fs::read_to_string(temp.path().join("updates.log"))
                    .unwrap()
                    .lines()
                    .count(),
                updates,
                "{name}"
            );
        }
    }
}
