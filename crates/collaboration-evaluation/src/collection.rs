//! Collect committed observations without scheduling workers or publishing knowledge.
use crate::{
    adjudication::apply_adjudications,
    artifacts::{immutable_write, private_path},
    bindings::{check_job, check_receipt},
    cards::report_cards,
    citations::{check_citation, regular_source, safe_source_path},
    identity::digest,
    metrics::{Metrics, summarize},
    planning::validate_plan,
    protocol::{Adjudications, Arm, CollectedClaim, Plan},
    usage::usage_trace,
};
use anyhow::{Result, ensure};
use config::{Config, safe_id};
use contracts::WorkflowState;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
    time::Duration,
};
use task::Job;
use workflow::Workflow;

pub(super) fn collect(
    c: &Config,
    plan: &Plan,
    grades: Option<&Adjudications>,
    mut status: impl FnMut(&str) -> Result<Value>,
) -> Result<Value> {
    let _deadline = process::deadline_scope(Duration::from_secs(120));
    let mut metrics = BTreeMap::from([
        (Arm::Baseline, Metrics::default()),
        (Arm::Candidate, Metrics::default()),
    ]);
    let mut claims = Vec::new();
    let mut known = BTreeSet::new();
    let mut tasks = Vec::new();
    let mut citation_cache = BTreeMap::<String, Option<String>>::new();
    for entry in &plan.tasks {
        let metric = metrics.get_mut(&entry.arm).unwrap();
        metric.scheduled_calls += 1;
        let job_file = c
            .state_dir
            .join("jobs")
            .join(format!("{}.json", entry.task.id));
        if !job_file.exists() {
            metric.incomplete_calls += 1;
            tasks.push(json!({"id":entry.task.id,"state":contracts::ExperimentTaskState::NotEnqueued,"usage":{"observed":null,"reason":"no registered run"}}));
            continue;
        }
        let job: Job = storage::read(&job_file)?;
        safe_id(&job.run_id)?;
        let dir = c.state_dir.join("runs").join(&job.run_id);
        let usage = usage_trace(&dir.join("process/stdout.jsonl"));
        if let Err(error) = check_job(c, plan, entry, &job) {
            metric.invalid_reports += 1;
            tasks.push(
                json!({"id":entry.task.id,"state":contracts::ExperimentTaskState::InvalidBinding,"error":format!("{error:#}"),"usage":usage}),
            );
            continue;
        }
        let state = status(&job.run_id)?;
        let terminal = state["status"]
            .as_str()
            .unwrap_or(WorkflowState::Unknown.as_str());
        let observed = workflow::parse_run_state(&state["status"]);
        if observed != Some(WorkflowState::Succeeded) {
            if matches!(
                observed,
                Some(WorkflowState::Failed | WorkflowState::Cancelled)
            ) {
                metric.failed_calls += 1;
            } else {
                metric.incomplete_calls += 1;
            }
            tasks.push(
                json!({"id":entry.task.id,"state":terminal,"attempt":job.attempt,"usage":usage}),
            );
            continue;
        }
        let decoded = (|| -> Result<Vec<CollectedClaim>> {
            let receipt: Value = storage::read(&dir.join("receipt.json"))?;
            check_receipt(c, plan, entry, &job, &state, &receipt)?;
            let cards = report_cards(entry, &receipt, plan.request.max_claims, &known)?;
            ensure!(
                cards
                    .iter()
                    .flat_map(|c| &c.card.evidence)
                    .any(|e| e.repository == "superpod"
                        && plan.request.knowledge_sources.contains(&e.path)
                        && check_citation(plan, e).is_ok()),
                "missing common SuperPOD content citation"
            );
            for card in &cards {
                if let Some(update) = &card.card.knowledge_update {
                    safe_source_path(&update.target_path)?;
                    ensure!(
                        update.target_path.starts_with("knowledge/")
                            && !update.proposal.trim().is_empty()
                            && update.proposal.len() <= 2048,
                        "invalid SuperPOD knowledge proposal"
                    );
                    let repo = &plan.inputs.repositories["superpod"];
                    regular_source(&repo.checkout, &repo.upstream.commit, &update.target_path)?;
                }
            }
            Ok(cards)
        })();
        match decoded {
            Ok(mut cards) => {
                metric.succeeded_calls += 1;
                for claim in &mut cards {
                    for citation in &claim.card.evidence {
                        let key = digest(citation)?;
                        let error = citation_cache.entry(key).or_insert_with(|| {
                            check_citation(plan, citation)
                                .err()
                                .map(|e| format!("{e:#}"))
                        });
                        if let Some(error) = error {
                            claim.citation_errors.push(error.clone());
                        }
                    }
                    known.insert(claim.id.clone());
                }
                claims.extend(cards);
                tasks.push(json!({"id":entry.task.id,"state":contracts::ExperimentTaskState::Succeeded,"attempt":job.attempt,"receipt_sha256":storage::digest(&fs::read(dir.join("receipt.json"))?),"usage":usage}));
            }
            Err(error) => {
                metric.invalid_reports += 1;
                tasks.push(json!({"id":entry.task.id,"state":contracts::ExperimentTaskState::InvalidReport,"error":format!("{error:#}"),"usage":usage}));
            }
        }
    }
    if let Some(grades) = grades {
        apply_adjudications(plan, &mut claims, grades)?;
    }
    Ok(summarize(plan, metrics, claims, tasks))
}

pub(super) fn execute(
    c: &Config,
    input: &Path,
    output: &Path,
    grades: Option<&Path>,
) -> Result<Value> {
    private_path(c, input, false)?;
    let plan: Plan = storage::read(input)?;
    validate_plan(c, &plan)?;
    let adjudications = grades
        .map(|path| {
            private_path(c, path, false)?;
            storage::read::<Adjudications>(path)
        })
        .transpose()?;
    let workflow = Workflow::new(
        &c.workflow,
        c.state_dir.join("workflow.sqlite"),
        c.state_dir.join("workflow"),
    );
    let result = collect(c, &plan, adjudications.as_ref(), |id| workflow.status(id))?;
    immutable_write(c, output, &result)?;
    Ok(result)
}

#[cfg(test)]
#[path = "collection_tests.rs"]
mod tests;
