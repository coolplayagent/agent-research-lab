//! Communication recovery uses only the host's retained membership authority.
use crate::now;
use config::Config;
use serde_json::json;
use task::{Job, Launch};

pub(super) fn communication_gap(c: &Config, job: &Job, reason: &str) {
    let _ = storage::write(
        &c.state_dir
            .join("communication/diagnostics")
            .join(format!("{}.json", job.run_id)),
        &json!({"run_id":job.run_id,"at":now(),"error":reason.chars().take(1024).collect::<String>(),"research_claims_verified":false}),
    );
}

pub(super) fn revoke_communication(c: &Config, job: &Job, launch: &Launch) {
    let member = launch
        .communication_member
        .as_ref()
        .or_else(|| launch.communication.as_ref().map(|v| &v.member));
    if let Some(member) = member
        && let Err(error) = multi_agent::revoke_member(&c.state_dir, member, now().max(0) as u64)
    {
        communication_gap(c, job, &error.to_string());
    }
}

#[cfg(test)]
#[path = "team_channel_tests.rs"]
mod tests;
