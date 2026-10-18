//! Recoverable completion journals coordinate memory and follow-up effects.
use crate::followups::admit_followups;
use crate::memory::{memory_call, memory_checkpoint};
use crate::receipts::authoritative_success_receipt;
use crate::team_channel::communication_gap;
use crate::{host_budget::now, job_store::job_path, people, scope::TaskScope};
use anyhow::{Context, Result, ensure};
use config::{Config, safe_id};
use contracts::{FollowupAdmission, FollowupState, MemoryWriteback, WorkflowState};
use multi_agent::TeamMembership;
use native_effects::fs;
use serde_json::{Value, json};
use std::{collections::BTreeSet, path::PathBuf};
use task::{Job, PostSuccess};
use workflow::Workflow;

/// Reject retired mutable formats before starting any worker or serving projections.
pub fn validate_state(c: &Config) -> Result<()> {
    ensure!(
        !c.state_dir.join("dashboard/personas.json").try_exists()?,
        "migration_required: use outcome-hub-migrate before starting the service"
    );
    let journals = c.state_dir.join("workflow/host-followups");
    if journals.try_exists()? {
        let mut count = 0;
        for entry in fs::read_dir(journals)? {
            let path = entry?.path();
            if path.extension().is_some_and(|e| e == "json") {
                count += 1;
                ensure!(count <= 100_000, "completion journal capacity exceeded");
                let record: PostSuccess = storage::read(&path).context(
                    "migration_required: completion journal requires offline conversion",
                )?;
                ensure!(
                    record.schema_version == 1
                        && path.file_stem().and_then(|s| s.to_str()) == Some(&record.run_id),
                    "invalid completion journal"
                );
            }
        }
    }
    Ok(())
}

pub(super) fn post_success_path(c: &Config, job: &Job) -> PathBuf {
    c.state_dir
        .join("workflow/host-followups")
        .join(format!("{}.json", job.run_id))
}

fn memory_gap(c: &Config, job: &Job, reason: &str) -> Result<()> {
    storage::write(
        &c.state_dir
            .join("runs")
            .join(&job.run_id)
            .join("memory-gap.json"),
        &json!({"error":reason,"worker_completed":true,"status":WorkflowState::Unknown,"automatic_retry":false}),
    )?;
    let mut current: Job = storage::read(&job_path(c, &job.task.id))?;
    ensure!(
        current.run_id == job.run_id,
        "memory gap belongs to an old run"
    );
    current.last_error = Some(format!("worker succeeded; memory writeback gap: {reason}"));
    storage::write(&job_path(c, &job.task.id), &current)
}

/// Reconstructible, private journal. Running memory writes are never replayed:
/// an interrupted subprocess may already have changed its external database.
pub(super) fn queue_post_success(c: &Config, w: &Workflow, job: &Job) -> Result<()> {
    let receipt = authoritative_success_receipt(w, job)?;
    if let Some(team) = &job.task.communication {
        if receipt.get("communication").is_some() {
            if let Err(error) =
                multi_agent::completed(&c.state_dir, job, team, &receipt, now().max(0) as u64)
            {
                communication_gap(c, job, &error.to_string());
            }
        } else {
            // A completed task may have continued independently after a partial
            // channel setup. Recover its membership intent even after launch cleared.
            let result = team.member(job).and_then(|member| {
                multi_agent::revoke_member(&c.state_dir, &member, now().max(0) as u64)
            });
            if let Err(error) = result {
                communication_gap(c, job, &error.to_string());
            }
        }
    }
    let digest = storage::digest(&serde_json::to_vec(&receipt)?);
    let path = post_success_path(c, job);
    let prior: Option<Value> = if path.exists() {
        Some(storage::read(&path)?)
    } else {
        None
    };
    let mut record = if let Some(value) = prior {
        let record: PostSuccess = serde_json::from_value(value)
            .context("migration_required: completion journal requires offline conversion")?;
        ensure!(
            record.schema_version == 1
                && record.task_id == job.task.id
                && record.run_id == job.run_id
                && record.receipt_sha256 == digest,
            "completion journal does not bind the committed receipt"
        );
        record
    } else {
        ensure!(
            receipt["post_success_protocol"] == 1,
            "migration_required: completion has no current side-effect journal"
        );
        PostSuccess {
            schema_version: 1,
            task_id: job.task.id.clone(),
            run_id: job.run_id.clone(),
            receipt_sha256: digest,
            persona_memory: if job.persona.is_some() {
                MemoryWriteback::Pending
            } else {
                MemoryWriteback::Done
            },
            persona_memory_error: None,
            memory: if job.task.use_memory {
                MemoryWriteback::Pending
            } else {
                MemoryWriteback::Done
            },
            followups: FollowupAdmission::Pending,
            memory_error: None,
            followup_error: None,
            followup_queued: vec![],
            followup_rejections: vec![],
        }
    };
    if record.memory == MemoryWriteback::Running {
        record.memory = MemoryWriteback::Unknown;
        record.memory_error = Some("controller interrupted during memory writeback; outcome unknown, automatic retry disabled".into());
    }
    // A resumed follow-up admission is safe: IDs are deterministic and enqueue
    // requires exact task equality before reusing any existing child.
    storage::write(&path, &record)?;
    storage::write(
        &c.state_dir
            .join("runs")
            .join(&job.run_id)
            .join("receipt.json"),
        &receipt,
    )?;
    if record.memory == MemoryWriteback::Unknown {
        memory_gap(
            c,
            job,
            record
                .memory_error
                .as_deref()
                .unwrap_or("memory outcome unknown"),
        )?;
    }
    Ok(())
}

pub(super) fn drain_followups_scoped(
    c: &Config,
    w: &Workflow,
    scope: Option<&TaskScope>,
    deferred: &mut BTreeSet<String>,
) -> Result<bool> {
    let queue = c.state_dir.join("workflow/host-followups");
    if !queue.exists() {
        return Ok(false);
    }
    let mut paths = fs::read_dir(&queue)?
        .map(|entry| entry.map(|e| e.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    paths.sort();
    let mut selected = None;
    for path in paths
        .into_iter()
        .filter(|p| p.extension().is_some_and(|s| s == "json"))
    {
        let record: PostSuccess = storage::read(&path)?;
        if scope.is_some_and(|scope| !scope.ids.contains(&record.task_id))
            || deferred.contains(&record.run_id)
        {
            continue;
        }
        if matches!(
            record.memory,
            MemoryWriteback::Pending | MemoryWriteback::Running
        ) || matches!(
            record.persona_memory,
            MemoryWriteback::Pending | MemoryWriteback::Running
        ) || matches!(
            record.followups,
            FollowupAdmission::Pending | FollowupAdmission::Running
        ) {
            selected = Some((path, record));
            break;
        }
    }
    let Some((path, mut record)) = selected else {
        return Ok(false);
    };
    safe_id(&record.task_id)?;
    let job: Job = storage::read(&job_path(c, &record.task_id))?;
    ensure!(
        record.schema_version == 1
            && record.run_id == job.run_id
            && job.launch.is_none()
            && path == post_success_path(c, &job),
        "completion record does not bind a settled task"
    );
    let receipt = authoritative_success_receipt(w, &job)?;
    ensure!(
        record.receipt_sha256 == storage::digest(&serde_json::to_vec(&receipt)?),
        "completion receipt differs from its private journal"
    );
    let dir = c.state_dir.join("runs").join(&job.run_id);
    if record.memory == MemoryWriteback::Running {
        record.memory = MemoryWriteback::Unknown;
        record.memory_error = Some(
            "interrupted memory writeback requires reconciliation; automatic retry disabled".into(),
        );
        storage::write(&path, &record)?;
        memory_gap(c, &job, record.memory_error.as_deref().unwrap())?;
    }
    if record.memory == MemoryWriteback::Pending {
        let checkpoint = memory_checkpoint(&job, &receipt)?;
        record.memory = MemoryWriteback::Running;
        storage::write(&path, &record)?;
        match memory_call(c, &job, &dir, "remember", Some(&checkpoint)) {
            Ok(_) => record.memory = MemoryWriteback::Done,
            Err(error) => {
                record.memory = MemoryWriteback::Unknown;
                record.memory_error = Some(error.to_string());
            }
        }
        storage::write(&path, &record)?;
        if record.memory == MemoryWriteback::Unknown {
            memory_gap(c, &job, record.memory_error.as_deref().unwrap())?;
        }
    }
    if matches!(
        record.persona_memory,
        MemoryWriteback::Pending | MemoryWriteback::Running
    ) {
        record.persona_memory = MemoryWriteback::Running;
        storage::write(&path, &record)?;
        match people::writeback(c, &job, &receipt) {
            Ok(_) => record.persona_memory = MemoryWriteback::Done,
            Err(error) => {
                record.persona_memory = MemoryWriteback::Unknown;
                record.persona_memory_error = Some(error.to_string());
                memory_gap(
                    c,
                    &job,
                    &format!("Persona memory writeback unconfirmed: {error}"),
                )?;
            }
        }
        storage::write(&path, &record)?;
    }
    if matches!(
        record.followups,
        FollowupAdmission::Pending | FollowupAdmission::Running
    ) {
        if let Some(scope) = scope
            && receipt["agent_report"]["next_tasks"]
                .as_array()
                .is_some_and(|tasks| !tasks.is_empty())
        {
            // Keep durable admission pending; a later unscoped controller may
            // handle it. This per-run set reports progress once, then allows exit.
            deferred.insert(job.run_id.clone());
            storage::write(
                &c.state_dir
                    .join("workflow/host-followups-deferred")
                    .join(format!("{}.json", job.run_id)),
                &json!({"status":FollowupState::DeferredTaskScope,"run_id":job.run_id,"task_id":job.task.id,"scope_sha256":scope.sha256,"reason":"fixed task scope cannot admit new follow-up tasks"}),
            )?;
            return Ok(true);
        }
        record.followups = FollowupAdmission::Running;
        storage::write(&path, &record)?;
        match admit_followups(c, &job, &dir, &receipt) {
            Ok(report) => {
                record.followups = FollowupAdmission::Done;
                record.followup_error = None;
                record.followup_queued = report.queued;
                record.followup_rejections = report.rejected;
            }
            Err(error) => {
                record.followup_error = Some(error.to_string());
                if process::deadline_exhausted() {
                    record.followups = FollowupAdmission::Pending;
                    storage::write(
                        &dir.join("followups-deferred.json"),
                        &json!({"status":FollowupState::DeferredBudget,"error":error.to_string()}),
                    )?;
                } else {
                    record.followups = FollowupAdmission::Blocked;
                    storage::write(
                        &dir.join("blocked-invalid-proposal.json"),
                        &json!({"status":FollowupState::FollowupBlocked,"error":error.to_string()}),
                    )?;
                }
            }
        }
        storage::write(&path, &record)?;
    }
    Ok(true)
}

#[cfg(test)]
#[path = "post_success_tests.rs"]
mod tests;
