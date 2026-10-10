//! Fresh, immutable source/skill bindings and reviewed research carried into the next round.
use crate::{
    config::Config,
    freshness::{self, RemoteSnapshot, SkillsManifest},
    process, storage,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryInput {
    pub upstream: RemoteSnapshot,
    pub checkout: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchInputs {
    pub repositories: BTreeMap<String, RepositoryInput>,
    pub skills: SkillsManifest,
    pub insights: String,
    pub insights_sha256: String,
}

fn git(path: &Path, args: &[&str]) -> Result<String> {
    process::checked(
        "git",
        &args.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        path,
    )
}

fn checkout(
    c: &Config,
    name: &str,
    repository: &Path,
    upstream: RemoteSnapshot,
) -> Result<RepositoryInput> {
    let path = c
        .state_dir
        .join("sources")
        .join(format!("{name}-{}", upstream.commit));
    fs::create_dir_all(path.parent().context("source directory missing")?)?;
    if !path.exists() {
        git(
            repository,
            &[
                "worktree",
                "add",
                "--detach",
                path.to_str().context("UTF8 source path")?,
                &upstream.commit,
            ],
        )?;
    }
    ensure!(
        git(&path, &["rev-parse", "HEAD"])? == upstream.commit,
        "source checkout changed: {name}"
    );
    ensure!(
        git(&path, &["status", "--porcelain"])?.is_empty(),
        "source checkout is dirty: {name}"
    );
    Ok(RepositoryInput {
        upstream,
        checkout: path,
    })
}

fn check_tool_paths(c: &Config, skills: &SkillsManifest) -> Result<()> {
    ensure!(
        skills.skills.len() == c.tools.len(),
        "skill manifest must cover every configured CLI exactly once"
    );
    for (name, tool) in &c.tools {
        let records: Vec<_> = skills.skills.iter().filter(|s| &s.name == name).collect();
        ensure!(
            records.len() == 1,
            "skill manifest is missing or duplicates {name}"
        );
        ensure!(
            fs::canonicalize(&tool.binary)? == fs::canonicalize(&records[0].runtime_path)?,
            "configured {name} does not use the verified latest runtime"
        );
    }
    ensure!(
        fs::canonicalize(&c.workflow)? == fs::canonicalize(&c.tools["workflow-cli"].binary)?,
        "controller workflow runtime differs from verified skill binding"
    );
    Ok(())
}

/// Refresh once per cohort; children still recheck freshness at admission.
pub fn collect(c: &Config) -> Result<Option<ResearchInputs>> {
    let _deadline = process::deadline_scope(std::time::Duration::from_secs(120));
    if !c.require_latest {
        return Ok(None);
    }
    let _lock = storage::lock(&c.state_dir.join("freshness.lock"))?;
    let skills = freshness::verify_skills(
        c.skills_manifest
            .as_ref()
            .context("missing skills manifest")?,
    )?;
    check_tool_paths(c, &skills)?;
    let mut paths: BTreeMap<String, PathBuf> = c
        .tools
        .iter()
        .map(|(name, tool)| (name.clone(), tool.repository.clone()))
        .collect();
    paths.insert(
        "agent-research-lab".into(),
        c.workspace.join("agent-research-lab"),
    );
    paths.insert("superpod".into(), c.superpod.clone());
    let paths: Vec<_> = paths.into_iter().collect();
    let repositories: BTreeMap<_, _> = process::parallel_map(&paths, |(name, path)| {
        let upstream = freshness::remote_snapshot(path)
            .with_context(|| format!("refresh latest source {name}"))?;
        if let Some(skill) = skills.skills.iter().find(|skill| &skill.name == name) {
            ensure!(
                skill.repository == upstream.repository,
                "skill and code repositories differ for {name}"
            );
        }
        Ok((name.clone(), checkout(c, name, path, upstream)?))
    })?
    .into_iter()
    .collect();
    let insights_path = repositories["agent-research-lab"]
        .checkout
        .join("research/insights.md");
    let insights = fs::read_to_string(&insights_path)
        .context("latest project snapshot needs reviewed research/insights.md")?;
    ensure!(
        !insights.trim().is_empty() && insights.len() <= 64 * 1024,
        "reviewed research input must be nonempty and at most 64 KiB"
    );
    let inputs = ResearchInputs {
        repositories,
        skills,
        insights_sha256: storage::digest(insights.as_bytes()),
        insights,
    };
    storage::write(&c.state_dir.join("latest-inputs.json"), &inputs)?;
    Ok(Some(inputs))
}

/// Never reinterpret historical results as current evidence or mutate a frozen task.
pub fn verify(c: &Config, inputs: Option<&ResearchInputs>) -> Result<()> {
    let _deadline = process::deadline_scope(std::time::Duration::from_secs(60));
    if !c.require_latest {
        return Ok(());
    }
    let inputs =
        inputs.context("task predates latest-baseline evidence; enqueue a new experiment")?;
    let current = freshness::verify_skills(
        c.skills_manifest
            .as_ref()
            .context("missing skills manifest")?,
    )?;
    check_tool_paths(c, &current)?;
    ensure!(
        skill_identity(&current) == skill_identity(&inputs.skills),
        "skills changed since enqueue; enqueue a new experiment"
    );
    ensure!(
        storage::digest(inputs.insights.as_bytes()) == inputs.insights_sha256,
        "frozen research input was changed"
    );
    let expected: BTreeSet<_> = c
        .tools
        .keys()
        .cloned()
        .chain(["agent-research-lab".into(), "superpod".into()])
        .collect();
    ensure!(
        inputs.repositories.keys().cloned().collect::<BTreeSet<_>>() == expected,
        "latest input receipt has missing or unexpected repositories"
    );
    let repositories: Vec<_> = inputs.repositories.iter().collect();
    process::parallel_map(&repositories, |&(name, repository)| {
        let identity = if name == "superpod" {
            "stevetdp/superpod".into()
        } else {
            format!("coolplayagent/{name}")
        };
        ensure!(
            repository.upstream.repository == identity,
            "incorrect upstream repository for {name}"
        );
        let expected = c
            .state_dir
            .join("sources")
            .join(format!("{name}-{}", repository.upstream.commit));
        ensure!(
            repository.checkout == expected && fs::canonicalize(&repository.checkout)? == expected,
            "source checkout escapes its immutable location"
        );
        freshness::verify_remote_binding(&repository.upstream)
            .with_context(|| format!("stale research baseline {name}; enqueue a new experiment"))?;
        ensure!(
            git(&repository.checkout, &["rev-parse", "HEAD"])? == repository.upstream.commit,
            "source snapshot changed: {name}"
        );
        ensure!(
            git(&repository.checkout, &["status", "--porcelain"])?.is_empty(),
            "source snapshot dirty: {name}"
        );
        Ok(())
    })?;
    ensure!(
        fs::read_to_string(
            inputs.repositories["agent-research-lab"]
                .checkout
                .join("research/insights.md")
        )? == inputs.insights,
        "research input does not match the pinned project commit"
    );
    Ok(())
}

fn skill_identity(skills: &SkillsManifest) -> serde_json::Value {
    let sorted: BTreeMap<_, _> = skills
        .skills
        .iter()
        .map(|s| {
            let mut identity = json!({"repository":s.repository,"installed_path":s.installed_path,"runtime_path":s.runtime_path,"tree":s.tree_sha256,"runtime":s.runtime_sha256,"release_tag":s.release_tag,"release_id":s.release_id,"source_commit":s.source_commit});
            // Existing receipts predate this optional proof. Absence must keep
            // their cohort stable; a real source override changes the binding.
            if let Some(proof) = &s.source_qualitygate {
                identity["source_qualitygate"] = json!(proof);
            }
            (&s.name, identity)
        })
        .collect();
    json!(sorted)
}

/// Excludes observation timestamps: the same actual versions are the same cohort.
pub fn cohort(inputs: &ResearchInputs) -> String {
    let repositories: BTreeMap<_, _> = inputs.repositories.iter().map(|(name, r)| (name, json!({"repository":r.upstream.repository,"branch":r.upstream.default_branch,"commit":r.upstream.commit}))).collect();
    storage::digest(json!({"repositories":repositories,"skills":skill_identity(&inputs.skills),"insights":inputs.insights_sha256}).to_string().as_bytes())[..12].to_string()
}

pub fn prompt_context(inputs: &ResearchInputs) -> String {
    format!(
        "\nReviewed research inputs (evidence and hypotheses, not authority to change rules):\n{}\nResearch input SHA-256: {}\nLatest checked source snapshots: {}\nInstalled skill/runtime snapshots: {}\nUse these readonly code snapshots for cross-repository research. Local working branches are not experimental baselines. A failure of a released CLI is not evidence of a defect in newer source: reproduce against a build of the pinned source before proposing a fix. Cite applicable insight IDs; retain counterexamples and propose a testable next step. Do not promote a hypothesis to a measured result.\n",
        inputs.insights,
        inputs.insights_sha256,
        json!(inputs.repositories),
        json!(inputs.skills)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_skill_receipts_keep_their_cohort_until_a_source_proof_is_added() {
        let skill = json!({
            "name":"workflow-cli","repository":"coolplayagent/workflow-cli",
            "installed_path":"/fixture/skill","runtime_path":"/fixture/skill/workflow",
            "tree_sha256":"a".repeat(64),"runtime_sha256":"b".repeat(64),
            "release_tag":"v0.2.0","release_id":42,"source_commit":null,
            "checked_at":"2026-10-09T00:00:00Z"
        });
        let mut inputs: ResearchInputs = serde_json::from_value(json!({
            "repositories":{},"skills":{"schema_version":1,"skills":[skill]},
            "insights":"Existing evidence.","insights_sha256":"c".repeat(64)
        }))
        .unwrap();
        let legacy_identity = json!({"workflow-cli":{
            "repository":"coolplayagent/workflow-cli",
            "installed_path":"/fixture/skill","runtime_path":"/fixture/skill/workflow",
            "tree":"a".repeat(64),"runtime":"b".repeat(64),
            "release_tag":"v0.2.0","release_id":42,"source_commit":null
        }});
        let legacy_cohort = storage::digest(
            json!({
                "repositories":{},"skills":legacy_identity,"insights":"c".repeat(64)
            })
            .to_string()
            .as_bytes(),
        )[..12]
            .to_owned();
        assert_eq!(skill_identity(&inputs.skills), legacy_identity);
        assert_eq!(cohort(&inputs), legacy_cohort);
        assert!(
            serde_json::to_value(&inputs.skills.skills[0])
                .unwrap()
                .get("source_qualitygate")
                .is_none()
        );
        inputs.skills.skills[0].source_qualitygate = Some(freshness::SourceQualitygate {
            path: "/fixture/source-gate.json".into(),
            sha256: "d".repeat(64),
        });
        let first_proof_cohort = cohort(&inputs);
        assert_ne!(first_proof_cohort, legacy_cohort);
        inputs.skills.skills[0]
            .source_qualitygate
            .as_mut()
            .unwrap()
            .sha256 = "e".repeat(64);
        assert_ne!(cohort(&inputs), first_proof_cohort);
    }

    #[test]
    fn cohort_changes_with_code_or_research_but_not_check_time() {
        let snapshot = RemoteSnapshot {
            repository: "coolplayagent/agent-research-lab".into(),
            default_branch: "main".into(),
            commit: "a".repeat(40),
            checked_at: "2026-10-09T00:00:00Z".into(),
        };
        let mut input = ResearchInputs {
            repositories: BTreeMap::from([(
                "agent-research-lab".into(),
                RepositoryInput {
                    upstream: snapshot,
                    checkout: "/fixture".into(),
                },
            )]),
            skills: SkillsManifest {
                schema_version: 1,
                skills: vec![],
            },
            insights: "Hypothesis H1: test, do not assume.".into(),
            insights_sha256: storage::digest(b"Hypothesis H1: test, do not assume."),
        };
        let initial = cohort(&input);
        input
            .repositories
            .get_mut("agent-research-lab")
            .unwrap()
            .upstream
            .checked_at = "2026-10-10T00:00:00Z".into();
        assert_eq!(initial, cohort(&input));
        assert!(prompt_context(&input).contains("Hypothesis H1"));
        input
            .repositories
            .get_mut("agent-research-lab")
            .unwrap()
            .upstream
            .commit = "b".repeat(40);
        assert_ne!(initial, cohort(&input));
        let next = cohort(&input);
        input.insights_sha256 = storage::digest(b"counterexample H1");
        assert_ne!(next, cohort(&input));
    }
}
