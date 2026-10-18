//! Frozen research prompts shared by registration and execution.
use crate::{followups::followup_instructions, people};
use anyhow::Result;
use config::Config;
use task::{Job, Task};

pub(super) fn role_prompt(c: &Config, task: &Task) -> Result<String> {
    let role = if let Some(version) = &task.prompt_version {
        let prompt = evolution_cli::resolve_registered_prompt(
            &c.state_dir.join("evolution/prompts.json"),
            version,
            &task.role,
        )?;
        prompt.definition().content.clone()
    } else {
        match task.role.as_str() {
            "implement" => include_str!("../../../prompts/implement.md"),
            "review" => include_str!("../../../prompts/critic.md"),
            "evaluate" | "evaluator" => include_str!("../../../prompts/evaluator.md"),
            _ => include_str!("../../../prompts/research.md"),
        }
        .to_owned()
    };
    Ok(role)
}

pub(super) fn rendered_task_prompt(c: &Config, task: &Task) -> Result<String> {
    let role = role_prompt(c, task)?;
    let mut prompt = format!(
        "{role}\n{}\n\n{}\n",
        task.prompt,
        followup_instructions(task),
    );
    if task.required_tools.iter().any(|t| t == "computer-use-cli") {
        prompt.push_str("Owned target experiment: use the installed computer-use CLI against DISPLAY/XAUTHORITY provided by the controller and the unique LAB_DESKTOP_WINDOW. Observe windows and screenshot first, then type a unique harmless token into this dedicated fixture and check LAB_DESKTOP_TARGET/observed.json. Task artifacts may be written under LAB_DESKTOP_TARGET; do not modify the repository. Do not connect to personal desktops. Missing target capability is an implementation finding to address, not evidence of success.\n");
    }
    if task.required_tools.iter().any(|t| t == "repo-sandbox") {
        prompt.push_str("Owned sandbox experiment: read targets-host.json in the task artifact directory (the parent directory of RELAY_MEMORY_HOME) for the host's real repo-sandbox evidence and retained registry. Do not attempt nested sandbox execution: this host restricts nested user namespaces. Missing target capability is an implementation finding to address, not evidence of success.\n");
    }
    Ok(prompt)
}

/// Render the same task and input context used by execution for a host-owned plan.
/// Rendering does not register a task, grant permissions or authorize execution.
pub fn rendered_experiment_prompt(
    c: &Config,
    task: &Task,
    inputs: Option<&inputs::ExecutionInputs>,
) -> Result<String> {
    let mut prompt = rendered_task_prompt(c, task)?;
    if let Some(inputs) = inputs {
        prompt.push_str(&inputs::prompt_context(inputs));
    }
    Ok(prompt)
}

pub fn rendered_job_prompt(c: &Config, job: &Job) -> Result<String> {
    Ok(format!(
        "{}{}",
        rendered_experiment_prompt(c, &job.task, job.research_inputs.as_ref())?,
        people::prompt(job.persona.as_ref())?
    ))
}

#[cfg(test)]
#[path = "prompts_tests.rs"]
mod tests;
