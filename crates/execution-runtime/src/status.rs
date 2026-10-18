//! Read-only controller projections and explicit capability probes.
use crate::git::git;
use crate::job_store::{jobs, workflow};
use crate::post_success::post_success_path;
use crate::worker_identity::process_start;
use anyhow::{Context, Result};
use config::Config;
use contracts::{ExecutionState, WorkflowNodeState, WorkflowState};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;
use task::{Job, PostSuccess};

pub fn doctor(c: &Config, probe_models: bool) -> Result<Value> {
    let latest_skills = if c.require_latest {
        Some(freshness::verify_skills(
            c.skills_manifest
                .as_ref()
                .context("missing skills manifest")?,
        )?)
    } else {
        None
    };
    let mut results = Vec::new();
    for (name, tool) in &c.tools {
        let result = process::capture(
            tool.binary.to_str().context("tool path")?,
            &tool.probe,
            &c.workspace,
            Duration::from_secs(20),
        );
        results.push(match result { Ok(out)=>json!({"tool":name,"available":out.status.success(),"repository":tool.repository,"version":String::from_utf8_lossy(&out.stdout).chars().take(500).collect::<String>()}),Err(e)=>json!({"tool":name,"available":false,"error":e.to_string()}) });
    }
    let knowledge = git(&c.superpod, &["rev-parse", "HEAD"]);
    let mut models = Vec::new();
    if probe_models {
        let mut observed = BTreeSet::new();
        for (role, model) in &c.models {
            let backend = c
                .role_backends
                .get(role)
                .map(String::as_str)
                .unwrap_or("codex");
            if !observed.insert((backend, model)) {
                continue;
            }
            models.push(match agent_backend::probe(c, role, model) {
                Ok(probe) => probe,
                Err(error) => json!({"model":model,"backend":backend,"available":false,"error":error.to_string()}),
            });
        }
    }
    Ok(
        json!({"agent_permissions":agent_policy::description(),"configured_backends":agent_backend::inventory(c),"role_backends":c.role_backends,"capability_status":"declared; model probes do not establish research quality or third-party compatibility","tools":results,"latest_skills":latest_skills,"superpod":{"path":c.superpod,"commit":knowledge.as_ref().ok(),"error":knowledge.err().map(|e|e.to_string())},"model_probes":models,"daily_seconds":c.daily_seconds,"max_agents":c.max_agents}),
    )
}

pub fn status(c: &Config) -> Result<Value> {
    let all_jobs = jobs(c)?;
    status_for_jobs(c, &all_jobs)
}

/// Aggregate the same controller states without retaining workflow receipts.
pub fn status_summary(c: &Config) -> Result<Value> {
    let all_jobs = jobs(c)?;
    observe_status(c, &all_jobs, false)
}

struct WorkflowStatusObservation {
    state: Option<WorkflowState>,
    paused: bool,
    task_state: Option<WorkflowNodeState>,
}

impl WorkflowStatusObservation {
    fn from_value(value: &Value) -> Self {
        Self {
            state: WorkflowState::from_value(&value["status"]),
            paused: value.get("pause").is_some_and(|v| !v.is_null()),
            task_state: WorkflowNodeState::from_value(
                &value["frames"]["1"]["nodes"]["task"]["state"]["state"],
            ),
        }
    }
}

/// Observe a caller-bounded job selection using the same controller state rules.
/// Missing dependencies remain explicit; callers must label incomplete selections.
pub fn status_for_jobs(c: &Config, all_jobs: &[Job]) -> Result<Value> {
    observe_status(c, all_jobs, true)
}

fn observe_status(c: &Config, all_jobs: &[Job], details: bool) -> Result<Value> {
    let w = workflow(c);
    let indexed: BTreeMap<_, _> = all_jobs.iter().map(|j| (j.task.id.clone(), j)).collect();
    let mut states = BTreeMap::new();
    let mut workflow_details = BTreeMap::new();
    let mut errors = BTreeMap::new();
    for job in all_jobs {
        match w.status(&job.run_id) {
            Ok(state) => {
                states.insert(
                    job.task.id.clone(),
                    WorkflowStatusObservation::from_value(&state),
                );
                // Summary observation drops each raw response here, before reading
                // the next workflow; only classification inputs cross iterations.
                if details {
                    workflow_details.insert(job.task.id.clone(), state);
                }
            }
            Err(error) => {
                if details {
                    errors.insert(job.task.id.clone(), error.to_string());
                }
            }
        }
    }
    let paused = c.state_dir.join("paused").exists();
    let mut result = Vec::new();
    let mut summary = BTreeMap::<ExecutionState, usize>::new();
    for job in all_jobs {
        let (state, reason) =
            controller_state(job, &indexed, &states, paused, &mut BTreeSet::new());
        *summary.entry(state).or_default() += 1;
        if !details {
            continue;
        }
        let postprocessing = if post_success_path(c, job).exists() {
            match storage::read::<PostSuccess>(&post_success_path(c, job)) {
                Ok(record) => {
                    json!({"memory":record.memory,"memory_error":record.memory_error,"persona_memory":record.persona_memory,"persona_memory_error":record.persona_memory_error,
                    "followups":record.followups,"followup_error":record.followup_error,
                    "queued":record.followup_queued,"rejected":record.followup_rejections})
                }
                Err(error) => json!({"error":error.to_string()}),
            }
        } else {
            Value::Null
        };
        result.push(json!({"id":job.task.id,"run_id":job.run_id,"model":job.model,
            "source_commit":job.source_commit,"superpod_commit":job.superpod_commit,
            "state":state,"reason":reason,"workflow_state":workflow_details.get(&job.task.id).map(|s| &s["status"]),
            "status":workflow_details.get(&job.task.id),"error":errors.get(&job.task.id),
            "last_error":job.last_error,"postprocessing":postprocessing}));
    }
    let ledger_path = c.state_dir.join("budget.json");
    let ledger: Value = if ledger_path.exists() {
        storage::read(&ledger_path)?
    } else {
        json!({})
    };
    if details {
        Ok(json!({"paused":paused,"summary":summary,"jobs":result,"budget":ledger}))
    } else {
        Ok(
            json!({"paused":paused,"summary":summary,"total_jobs":all_jobs.len(),"details_included":false,"budget":ledger}),
        )
    }
}

/// Workflow `running` also includes task_ready. Only a matching retained live
/// process is controller execution; pending work must expose its actual blocker.
fn controller_state(
    job: &Job,
    indexed: &BTreeMap<String, &Job>,
    states: &BTreeMap<String, WorkflowStatusObservation>,
    paused: bool,
    visiting: &mut BTreeSet<String>,
) -> (ExecutionState, Option<String>) {
    if !visiting.insert(job.task.id.clone()) {
        return (
            ExecutionState::Blocked,
            Some("dependency cycle requires reconciliation".into()),
        );
    }
    let result = (|| {
        let Some(workflow) = states.get(&job.task.id) else {
            return (
                ExecutionState::Unknown,
                Some("workflow status unavailable; inspect error".into()),
            );
        };
        if workflow.state.is_none() {
            return (
                ExecutionState::Unknown,
                Some("unrecognized workflow state".into()),
            );
        }
        if workflow.state == Some(WorkflowState::Succeeded) {
            return (ExecutionState::Succeeded, None);
        }
        if let Some(launch) = &job.launch {
            if launch.pid > 0
                && process_start(launch.pid).ok().as_ref() == Some(&launch.process_start)
            {
                return (ExecutionState::Running, None);
            }
            return (
                ExecutionState::NeedsReconciliation,
                Some("retained launch has no matching live process".into()),
            );
        }
        if let Some(due) = job.retry_after {
            return (
                ExecutionState::RetryWait,
                Some(format!("bounded read retry due at {due}")),
            );
        }
        if let Some(error) = &job.last_error {
            return (ExecutionState::Blocked, Some(error.clone()));
        }
        if workflow.state == Some(WorkflowState::Failed)
            || workflow.state == Some(WorkflowState::Cancelled)
        {
            return (
                ExecutionState::Failed,
                Some("workflow ended without a successful receipt".into()),
            );
        }
        if workflow.paused {
            return (ExecutionState::Paused, Some("workflow is paused".into()));
        }
        let mut waiting = vec![];
        for id in &job.task.dependencies {
            let Some(dependency) = indexed.get(id) else {
                return (
                    ExecutionState::Blocked,
                    Some(format!("dependency {id} is missing")),
                );
            };
            let (state, reason) = controller_state(dependency, indexed, states, paused, visiting);
            if [
                ExecutionState::Blocked,
                ExecutionState::Failed,
                ExecutionState::NeedsReconciliation,
                ExecutionState::Unknown,
            ]
            .contains(&state)
            {
                return (
                    ExecutionState::Blocked,
                    Some(format!(
                        "dependency {id} is {state}: {}",
                        reason.unwrap_or_default()
                    )),
                );
            }
            if state != ExecutionState::Succeeded {
                waiting.push(id.as_str());
            }
        }
        if !waiting.is_empty() {
            return (
                ExecutionState::WaitingDependencies,
                Some(format!("awaiting {}", waiting.join(", "))),
            );
        }
        if paused {
            return (
                ExecutionState::Paused,
                Some("controller admission is paused".into()),
            );
        }
        if workflow.task_state == Some(WorkflowNodeState::TaskReady) {
            return (ExecutionState::Ready, None);
        }
        (
            ExecutionState::NeedsReconciliation,
            Some("workflow has no ready task or matching retained launch".into()),
        )
    })();
    visiting.remove(&job.task.id);
    result
}

#[cfg(test)]
#[path = "status_tests.rs"]
mod tests;
