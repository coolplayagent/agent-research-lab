//! Optional research cohort creation from frozen external inputs.
use crate::host_budget::{RunWindow, idle_host, load_ledger, now};
use crate::jobs::enqueue_with_inputs;
use crate::people;
use anyhow::{Context, Result};
use config::Config;
use serde_json::{Value, json};
use task::Task;

pub(super) fn seed_inputs(c: &Config) -> Result<Value> {
    let day = budget::day(now());
    let inputs = inputs::collect(c)?;
    let config_id = storage::digest(&serde_json::to_vec(c)?);
    let roster = people::directory(c)?;
    let roster_input: Vec<_> = roster
        .defaults
        .iter()
        .map(|(role, id)| (role, &roster.people[id]))
        .collect();
    let roster_digest = storage::digest(&serde_json::to_vec(&roster_input)?);
    let base_round = inputs
        .as_ref()
        .map(|v| format!("{day}-{}-{}", inputs::cohort(v), &config_id[..6]))
        .unwrap_or_else(|| day.to_string());
    let round = format!("{base_round}-{}", &roster_digest[..8]);
    let shared = c.multi_agent.as_ref().is_some_and(|v| v.shared_research);
    let mut ids = Vec::new();
    for (topic, title) in [
        (
            "sdlc",
            "verifiable automated delivery from requirements to PR",
        ),
        (
            "memory",
            "cross-session memory and long-running task recovery",
        ),
        (
            "collaboration",
            "limits of diverse viewpoints and multi-agent collaboration",
        ),
        (
            "computer",
            "Computer Use and recovery of sandbox operations",
        ),
    ] {
        let communication = shared.then(|| multi_agent::TeamBinding {
            id: format!("research-{round}"),
            cell: Some(topic.into()),
            topics: vec![topic.into()],
            query: String::new(),
        });
        let independent_phase = if shared {
            "Discuss this research topic. Form an independent argument before reading peers. Respond to a specific claim with supporting evidence, objections, counterexamples or questions; link it with reply_to. Do not post disconnected reports. If no one has spoken, open with a grounded observation and open question. After independent work, read at most once more and respond to valuable new ideas; do not wait or auto-loop. Explain how new angles connect to the topic and return to testable questions. Preserve disagreement instead of seeking consensus."
        } else {
            "Do not read other agents conclusions during this phase."
        };
        let independent = format!(
            "Research {title}. Analyze the pinned SuperPOD commit read-only; produce sourced hypotheses with reproducible experiments. Identify actual usage gaps in the seven coolplayagent CLIs. Do not modify files, install, push, send messages or merge. next_tasks must be empty in this phase; put follow-up experiment suggestions in findings for synthesis to select. Return the specified JSON schema."
        );
        let independent = if shared {
            independent.replace(
                "Do not modify files, install, push, send messages or merge.",
                "Do not modify source, install, push, message external contacts or merge.",
            )
        } else {
            independent
        };
        let research_id = format!("research-{round}-{topic}");
        let critic_id = format!("critic-{round}-{topic}");
        let synthesis_id = format!("synthesis-{round}-{topic}");
        for (id, role, prompt) in [
            (
                research_id.clone(),
                "research",
                format!("{independent} Present an independent argument; {independent_phase}"),
            ),
            (
                critic_id.clone(),
                "review",
                format!(
                    "{independent} Develop alternative explanations, counterexamples and failure conditions as an independent critic; {independent_phase}"
                ),
            ),
        ] {
            enqueue_with_inputs(
                c,
                Task {
                    persona_id: Some(roster.defaults[role].clone()),
                    id: id.clone(),
                    role: role.into(),
                    repository: "superpod".into(),
                    prompt,
                    prompt_version: None,
                    communication: communication.clone(),
                    max_attempts: None,
                    use_memory: false,
                    depth: 0,
                    write: false,
                    exploratory: topic == "collaboration",
                    dependencies: vec![],
                    required_tools: if topic == "computer" && c.require_latest {
                        vec!["computer-use-cli".into(), "repo-sandbox".into()]
                    } else {
                        vec![]
                    },
                },
                inputs.clone(),
            )?;
            ids.push(id);
        }
        enqueue_with_inputs(
            c,
            Task {
                persona_id: Some(roster.defaults["synthesis"].clone()),
                id: synthesis_id.clone(),
                role: "research".into(),
                repository: "superpod".into(),
                prompt: format!(
                    "Synthesize the two independent investigations and critiques of {title}. Preserve minority views, counterexamples and unresolved disagreements; verify citations and propose bounded experiments that distinguish competing hypotheses. Untested capabilities remain hypotheses. Consensus cannot replace evidence. With team communication, respond first to a specific disagreement or open question, then connect perspectives and explain which new angles merit further investigation. Use reply_to; do not invent peers responses or post a disconnected monologue. Based on original disagreement evidence, propose at most three bounded experiments or CLI fixes in next_tasks; return an empty array without sufficient evidence. Do not modify files, install, push or merge."
                ),
                prompt_version: None,
                communication: communication.clone(),
                max_attempts: None,
                use_memory: false,
                depth: 0,
                write: false,
                exploratory: topic == "collaboration",
                dependencies: vec![research_id, critic_id],
                required_tools: vec![],
            },
            inputs.clone(),
        )?;
        ids.push(synthesis_id);
    }
    Ok(json!({"queued":ids}))
}

pub fn seed(c: &Config) -> Result<Value> {
    let _lock = storage::lock(&c.state_dir.join("controller.lock"))?;
    let mut ledger = load_ledger(c)?;
    idle_host(
        c,
        &mut ledger,
        RunWindow {
            started: now(),
            max_seconds: 0,
        },
        || seed_inputs(c),
    )?
    .context("seed paused or daily budget exhausted")
}
