//! Settle one observed worker attempt against its retained execution lease.
use crate::{candidate::capture_candidate, receipts::validate_report};
use anyhow::{Context, Result, ensure};
use contracts::WorkflowState;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::Duration;
use task::Job;
use workflow::Workflow;

#[derive(Serialize, Deserialize)]
pub(super) struct ExitObservation {
    pub(super) success: bool,
    pub(super) code: Option<i32>,
    pub(super) reason: Option<String>,
}

/// Settle actual observations, including malformed success output as failure/uncertainty.
pub(super) fn settle(w: &Workflow, job: &Job, observation: &ExitObservation) -> Result<bool> {
    settle_with_heartbeat(w, job, observation, None)
}

fn with_execution_lease<T>(
    retained: &Value,
    heartbeat: Option<&lease_heartbeat::LeaseHeartbeat>,
    operation: impl FnOnce(&Value) -> Result<T>,
) -> Result<T> {
    if let Some(heartbeat) = heartbeat {
        heartbeat.with_lease(operation)
    } else {
        let _deadline = process::deadline_scope(Duration::from_secs(10));
        operation(retained)
    }
}

pub(super) fn settle_with_heartbeat(
    w: &Workflow,
    job: &Job,
    observation: &ExitObservation,
    heartbeat: Option<&lease_heartbeat::LeaseHeartbeat>,
) -> Result<bool> {
    let launch = job.launch.as_ref().context("missing launch receipt")?;
    let valid_report = (|| -> Result<Value> {
        if let Some(inputs) = &job.research_inputs {
            freshness::verify_installed_bytes(&inputs.skills)?;
        }
        ensure!(
            observation.success,
            "worker failed: {:?}; exit {:?}",
            observation.reason,
            observation.code
        );
        let value = agent_backend::result(
            job.backend.as_ref(),
            &launch.log_dir,
            launch.backend_request_sha256.as_deref(),
        )?;
        validate_report(&value)?;
        Ok(value)
    })();
    match valid_report {
        Ok(value) => {
            ensure!(
                launch.tools.is_object(),
                "missing private launch tool snapshot"
            );
            let candidate = if job.task.write {
                match capture_candidate(w, job) {
                    Ok(candidate) => Some(candidate),
                    Err(error) => {
                        with_execution_lease(&launch.lease, heartbeat, |lease| {
                            w.unknown_effect(
                                lease,
                                &launch.attempt,
                                &format!("candidate snapshot rejected: {error:#}"),
                            )
                        })?;
                        storage::write(
                            &launch.log_dir.join("rejected-result.json"),
                            &json!({"reason":error.to_string()}),
                        )?;
                        return Ok(false);
                    }
                }
            } else {
                None
            };
            let mut receipt = json!({"receipt_protocol":2,"config_digest":job.config_digest,"agent_report":value,"source_commit":job.source_commit,"superpod_commit":job.superpod_commit,"prompt_digest":job.prompt_digest,"model":job.model,"tools":launch.tools,"research_inputs":job.research_inputs,"candidate":candidate,"post_success_protocol":1,"claim":"process completed; research claims require independent evaluation"});
            if let Some(backend) = &job.backend {
                receipt["backend"] = json!(backend);
                if let Some(digest) = &launch.backend_request_sha256 {
                    receipt["backend_request_sha256"] = json!(digest);
                }
            }
            if let Some(context) = &launch.communication {
                receipt["communication"] = serde_json::to_value(context)?;
            }
            if let Some(gap) = &launch.communication_gap {
                receipt["communication_gap"] = json!(gap);
            }
            let receipt_path = launch.log_dir.join("receipt.json");
            storage::write(&receipt_path, &receipt)?;
            let outputs = json!({"result":serde_json::to_string(&receipt)?});
            if job.task.write {
                with_execution_lease(&launch.lease, heartbeat, |lease| {
                    w.finish_effect(
                        lease,
                        &launch.attempt,
                        &job.worktree.display().to_string(),
                        &receipt_path,
                        outputs,
                    )
                })?;
            } else {
                // Keep the exact completion time and result for a lost-reply retry.
                let result_path = w
                    .work_dir
                    .join("host-results")
                    .join(format!("{}.json", job.run_id));
                let result: Value = if result_path.exists() {
                    storage::read(&result_path)?
                } else {
                    let result = json!({"protocol_version":1,"request_digest":launch.attempt["grant"]["request_digest"],"completed_at_unix_ms":workflow::now_ms()?,"outcome":{"status":WorkflowState::Succeeded,"outputs":outputs,"evidence":[]}});
                    storage::write(&result_path, &result)?;
                    result
                };
                with_execution_lease(&launch.lease, heartbeat, |lease| {
                    w.finish_result(lease, &launch.attempt, &result)
                })?;
            }
        }
        Err(error) => {
            let reason = format!("worker outcome rejected: {error:#}");
            if job.task.write {
                with_execution_lease(&launch.lease, heartbeat, |lease| {
                    w.unknown_effect(lease, &launch.attempt, &reason)
                })?;
            } else {
                with_execution_lease(&launch.lease, heartbeat, |lease| {
                    w.fail(lease, &launch.attempt, &reason)
                })?;
            }
            storage::write(
                &launch.log_dir.join("rejected-result.json"),
                &json!({"reason":reason}),
            )?;
            return Ok(false);
        }
    }
    Ok(true)
}
