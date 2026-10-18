//! Follow-up authority, schema and deterministic child registration.
use crate::{job_store::job_path, jobs::enqueue_with_inputs, people};
use anyhow::{Context, Result, ensure};
use config::Config;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeSet, path::Path};
use task::{Job, ProposalRejection, Task};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NextTask {
    repository: String,
    role: String,
    prompt: String,
    write: bool,
    exploratory: bool,
    #[serde(default)]
    required_tools: Vec<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub(super) struct FollowupReport {
    pub(super) queued: Vec<String>,
    pub(super) rejected: Vec<ProposalRejection>,
}

fn may_propose_followups(task: &Task) -> bool {
    task.depth < 3 && (task.id.starts_with("synthesis-") || task.role == "implement")
}

pub(super) fn followup_instructions(task: &Task) -> &'static str {
    if !may_propose_followups(task) {
        "Follow-up authority: next_tasks must be []. Put proposed experiments and unresolved questions in findings; this task cannot enqueue follow-ups."
    } else if task.role == "implement" {
        "Follow-up authority: propose at most three independent reviews with role=review and write=false. Do not request more implementation or change any role to obtain write permission. Declare required_tools explicitly: real GUI experiments need computer-use-cli, sandbox experiments need repo-sandbox, and tasks needing neither use []. Missing tool or target bindings are a functional gap, not evidence of success. Return [] when no review is justified."
    } else {
        "Follow-up authority: propose at most three bounded tasks. Read-only research/review must set write=false. A justified code or document change must explicitly use role=implement and write=true; never label a writing task as research/review. Keep the actual task description and write flag consistent. The host will not upgrade roles or permissions. Declare required_tools explicitly: real GUI experiments need computer-use-cli, sandbox experiments need repo-sandbox, and tasks needing neither use []. Missing tool or target bindings are a functional gap, not evidence of success. Follow-ups do not authorize installation, publishing or merging. Return [] when evidence does not justify a task."
    }
}

pub(super) fn report_schema(c: &Config, task: &Task) -> Value {
    let tools: Vec<_> = c.tools.keys().cloned().collect();
    let repositories: Vec<_> = c
        .tools
        .keys()
        .cloned()
        .chain(["superpod".into(), "agent-research-lab".into()])
        .collect();
    let proposal = |roles: Vec<&str>, write_values: Vec<bool>| {
        json!({
            "type":"object","properties":{
                "repository":{"type":"string","enum":repositories},
                "role":{"type":"string","enum":roles},
                "prompt":{"type":"string"},"write":{"type":"boolean","enum":write_values},
                "exploratory":{"type":"boolean"},
                "required_tools":{"type":"array","maxItems":7,"items":{"type":"string","enum":tools}}},
            "required":["repository","role","prompt","write","exploratory","required_tools"],
            "additionalProperties":false
        })
    };
    let items = if task.role == "implement" {
        proposal(vec!["review"], vec![false])
    } else {
        json!({"anyOf":[
            proposal(vec!["research", "review"], vec![false]),
            proposal(vec!["implement"], vec![false, true])
        ]})
    };
    json!({"type":"object","properties":{
        "summary":{"type":"string"},"findings":{"type":"array","items":{"type":"string"}},
        "sources":{"type":"array","items":{"type":"string"}},"limitations":{"type":"array","items":{"type":"string"}},
        "next_tasks":{"type":"array","maxItems":if may_propose_followups(task) {3} else {0},"items":items}
    },"required":["summary","findings","sources","limitations","next_tasks"],"additionalProperties":false})
}

fn validate_proposal(c: &Config, parent: &Task, index: usize, proposal: &NextTask) -> Result<()> {
    ensure!(
        parent.depth < 3 && index < 3,
        "follow-up limit or maximum depth reached"
    );
    ensure!(
        may_propose_followups(parent),
        "only synthesis and implementation may propose follow-ups"
    );
    ensure!(
        ["research", "implement", "review"].contains(&proposal.role.as_str()),
        "unsupported follow-up role"
    );
    ensure!(
        !proposal.prompt.trim().is_empty() && proposal.prompt.len() <= 16000,
        "invalid follow-up prompt"
    );
    ensure!(
        c.tools.contains_key(&proposal.repository)
            || ["superpod", "agent-research-lab"].contains(&proposal.repository.as_str()),
        "follow-up repository not allowlisted"
    );
    ensure!(
        !proposal.write || proposal.role == "implement",
        "only implementation follow-ups can write"
    );
    ensure!(
        parent.role != "implement" || (proposal.role == "review" && !proposal.write),
        "implementation can only request independent read-only review"
    );
    ensure!(
        proposal.required_tools.len() <= 7,
        "at most seven required tools"
    );
    let mut tools = BTreeSet::new();
    for tool in &proposal.required_tools {
        ensure!(c.tools.contains_key(tool), "unknown required tool {tool}");
        ensure!(tools.insert(tool), "duplicate required tool {tool}");
    }
    Ok(())
}

pub(super) fn admit_followups(
    c: &Config,
    parent: &Job,
    dir: &Path,
    receipt: &Value,
) -> Result<FollowupReport> {
    let empty = vec![];
    let proposals = match receipt["agent_report"].get("next_tasks") {
        Some(value) => value.as_array().context("next_tasks must be an array")?,
        None => &empty, // Historical receipts may predate follow-up proposals.
    };
    let mut report = FollowupReport::default();
    let mut admissible = Vec::new();
    for (index, value) in proposals.iter().enumerate() {
        let parsed = serde_json::from_value::<NextTask>(value.clone())
            .context("malformed follow-up proposal")
            .and_then(|proposal| {
                validate_proposal(c, &parent.task, index, &proposal)?;
                Ok(proposal)
            });
        match parsed {
            Ok(proposal) => admissible.push((index, proposal)),
            Err(error) => report.rejected.push(ProposalRejection {
                index,
                reason: format!("{error:#}"),
            }),
        }
    }
    // Persist each proposal's disposition without treating one bad sibling as
    // authority to discard a valid one. Original indices are stable retry IDs.
    storage::write(&dir.join("followups.json"), &report)?;
    let inputs = if admissible.is_empty() {
        None
    } else {
        inputs::collect(c)?
    };
    for (index, proposal) in admissible {
        let id = format!(
            "followup-{}-{index}",
            &storage::digest(parent.run_id.as_bytes())[..16]
        );
        enqueue_with_inputs(
            c,
            Task {
                persona_id: if let Some(prior) = job_path(c, &id)
                    .exists()
                    .then(|| storage::read::<Job>(&job_path(c, &id)))
                    .transpose()?
                {
                    prior.task.persona_id
                } else if parent.persona.is_some() {
                    Some(people::default_id(c, &proposal.role)?)
                } else {
                    None
                },
                id: id.clone(),
                role: proposal.role,
                repository: proposal.repository,
                prompt: proposal.prompt,
                prompt_version: None,
                communication: parent.task.communication.clone(),
                max_attempts: None,
                use_memory: false,
                depth: parent.task.depth + 1,
                write: proposal.write,
                exploratory: proposal.exploratory,
                dependencies: vec![parent.task.id.clone()],
                required_tools: proposal.required_tools,
            },
            inputs.clone(),
        )?;
        report.queued.push(id);
        storage::write(&dir.join("followups.json"), &report)?;
    }
    Ok(report)
}

#[cfg(test)]
#[path = "followups_tests.rs"]
mod tests;
