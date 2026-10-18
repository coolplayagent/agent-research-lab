//! Host-owned candidate capture preserves protected paths and frozen Git authority.
use crate::git;
use anyhow::{Context, Result, ensure};
use native_effects::fs;
use serde_json::{Value, json};
use std::{collections::BTreeSet, time::Duration};
use task::Job;
use workflow::Workflow;

fn forbidden_candidate_path(path: &str) -> bool {
    path.split('/').any(|part| {
        part == ".git"
            || part == "AGENTS.md"
            || part == "auth.json"
            || part == "credentials.json"
            || part == "id_rsa"
            || part.starts_with(".env")
            || part == "private"
            || part == "holdouts"
            || part.ends_with(".pem")
            || part.ends_with(".key")
    }) || path.starts_with(".github/")
        || path.starts_with(".qualitygate/")
        || path.starts_with("qualitygate/")
        || path == "qualitygate.yaml"
        || path == "qualitygate.yml"
}

pub(super) fn capture_candidate(w: &Workflow, job: &Job) -> Result<Value> {
    let launch = job.launch.as_ref().context("missing write launch")?;
    ensure!(
        git(&job.worktree, &["rev-parse", "--absolute-git-dir"])? == launch.git_dir,
        "agent changed worktree Git authority"
    );
    let retained = w
        .work_dir
        .join("candidates")
        .join(format!("{}.json", job.run_id));
    if retained.exists() {
        let snapshot: Value = storage::read(&retained)?;
        ensure!(
            snapshot["candidate_commit"] == git(&job.worktree, &["rev-parse", "HEAD"])?,
            "retained candidate changed"
        );
        return Ok(snapshot);
    }
    ensure!(
        git(&job.worktree, &["rev-parse", "HEAD"])? == job.source_commit,
        "agent committed or changed baseline outside host snapshot"
    );
    let detached = process::capture(
        "git",
        &["symbolic-ref".into(), "-q".into(), "HEAD".into()],
        &job.worktree,
        Duration::from_secs(10),
    )?;
    ensure!(
        !detached.status.success(),
        "candidate worktree must remain detached"
    );
    let mut changed = BTreeSet::new();
    for args in [
        vec!["diff", "--name-only", "-z", "HEAD"],
        vec!["ls-files", "--others", "--exclude-standard", "-z"],
    ] {
        let out = process::capture(
            "git",
            &args.into_iter().map(String::from).collect::<Vec<_>>(),
            &job.worktree,
            Duration::from_secs(20),
        )?;
        ensure!(out.status.success(), "cannot inspect candidate files");
        for path in out.stdout.split(|b| *b == 0).filter(|b| !b.is_empty()) {
            let path = std::str::from_utf8(path)?;
            ensure!(
                !forbidden_candidate_path(path),
                "candidate changes protected or credential path: {path}"
            );
            changed.insert(path.to_owned());
        }
    }
    let mut files = vec![];
    for path in &changed {
        let full = job.worktree.join(path);
        if let Ok(metadata) = fs::symlink_metadata(&full) {
            ensure!(
                metadata.is_file() && !metadata.file_type().is_symlink(),
                "candidate changes nonregular file: {path}"
            );
            ensure!(
                metadata.len() <= 64 * 1024 * 1024,
                "candidate file exceeds 64 MiB: {path}"
            );
            files.push(json!({"path":path,"sha256":storage::digest(&fs::read(full)?)}));
        } else {
            files.push(json!({"path":path,"deleted":true}));
        }
    }
    if !changed.is_empty() {
        git(&job.worktree, &["add", "-A"])?;
        git(
            &job.worktree,
            &[
                "-c",
                "core.hooksPath=/dev/null",
                "-c",
                "user.name=Agent Research Lab",
                "-c",
                "user.email=agent-research-lab@users.noreply.github.com",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-m",
                &format!("research candidate: {}", job.task.id),
            ],
        )?;
    }
    let candidate = git(&job.worktree, &["rev-parse", "HEAD"])?;
    let diff = process::capture(
        "git",
        &[
            "diff".into(),
            "--binary".into(),
            job.source_commit.clone(),
            candidate.clone(),
        ],
        &job.worktree,
        Duration::from_secs(20),
    )?;
    ensure!(diff.status.success(), "candidate diff failed");
    let diff_dir = w.work_dir.join("candidates");
    fs::create_dir_all(&diff_dir)?;
    fs::write(diff_dir.join(format!("{}.patch", job.run_id)), &diff.stdout)?;
    let snapshot = json!({"baseline_commit":job.source_commit,"candidate_commit":candidate,"no_change":changed.is_empty(),"files":files,"diff_sha256":storage::digest(&diff.stdout)});
    storage::write(&retained, &snapshot)?;
    Ok(snapshot)
}

#[cfg(test)]
#[path = "candidate_tests.rs"]
mod tests;
