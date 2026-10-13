//! Read-only owner confirmation. Missing is recognized only by a typed owner code.
use super::*;
use contracts::{
    WorkflowFrameState, WorkflowLookupErrorCode, WorkflowNodeState, WorkflowObservationErrorCode,
    WorkflowState,
};

const OWNER_MESSAGE_LIMIT: usize = 2_097_152;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistrationProof {
    pub run_id: String,
    pub revision: u64,
    pub state: WorkflowState,
    pub state_digest: String,
    pub bundle_digest: String,
    pub verification_sha256: String,
    pub history_sha256: String,
    pub kernel_result_sha256: String,
}

#[derive(Debug)]
pub struct ObservationError {
    pub code: WorkflowObservationErrorCode,
    pub message: String,
}
impl std::fmt::Display for ObservationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for ObservationError {}
fn observation_error(code: WorkflowObservationErrorCode, error: anyhow::Error) -> anyhow::Error {
    if error.is::<ObservationError>() {
        error
    } else {
        ObservationError {
            code,
            message: format!("{error:#}"),
        }
        .into()
    }
}

impl Workflow {
    /// Validate an interrupted first-start request without invoking any mutation.
    pub fn validate_retained_task_start(
        &self,
        run_id: &str,
        write: bool,
        payload: Value,
        timeout_ms: u64,
    ) -> Result<()> {
        let path = self
            .work_dir
            .join(format!("start-{}.json", hex(run_id.as_bytes())));
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
            Ok(_) => {}
        }
        let retained: Value = storage::read(&path)?;
        let timestamp = retained["started_at_unix_ms"]
            .as_u64()
            .context("retained start timestamp missing")?;
        let bundle = task_bundle(
            &[TaskSpec {
                id: "task".into(),
                payload,
                write,
                timeout_ms,
            }],
            false,
        )?;
        ensure!(
            retained
                == json!({"schema_version":1,"bundle":bundle,"run_id":run_id,
            "inputs":{},"started_at_unix_ms":timestamp}),
            "retained start binding mismatch"
        );
        Ok(())
    }

    fn observe_run(&self, args: &[String]) -> Result<Option<(Value, String)>> {
        self.observe_run_inner(args)
            .map_err(|error| observation_error(WorkflowObservationErrorCode::Unavailable, error))
    }
    fn observe_run_inner(&self, args: &[String]) -> Result<Option<(Value, String)>> {
        let output = process::capture(
            self.binary.to_str().context("non-UTF8 workflow binary")?,
            args,
            &self.work_dir,
            Duration::from_secs(30),
        )?;
        ensure!(
            output.stdout.len() <= OWNER_MESSAGE_LIMIT,
            "owner message exceeds protocol limit"
        );
        let value: Value = serde_json::from_slice(&output.stdout).context("invalid owner JSON")?;
        if !output.status.success() {
            if value["ok"] == false
                && value.get("commit_may_have_succeeded").is_none()
                && WorkflowLookupErrorCode::from_value(&value["error"]["code"])
                    == Some(WorkflowLookupErrorCode::NotFound)
            {
                return Ok(None);
            }
            bail!("owner observation unavailable (exit {})", output.status);
        }
        ensure!(
            value["ok"] == true,
            "owner observation lacks success envelope"
        );
        Ok(Some((
            value.get("result").context("owner result missing")?.clone(),
            hex(&output.stdout),
        )))
    }

    /// Confirm the exact current run, without initialize/start/lease/drive or DB access.
    /// None is a positively identified missing run; every other failure remains an error.
    pub fn confirm_registration(
        &self,
        run_id: &str,
        write: bool,
        payload: Value,
        timeout_ms: u64,
    ) -> Result<Option<RegistrationProof>> {
        self.confirm_registration_inner(run_id, write, payload, timeout_ms)
            .map_err(|error| observation_error(WorkflowObservationErrorCode::Mismatch, error))
    }
    fn confirm_registration_inner(
        &self,
        run_id: &str,
        write: bool,
        payload: Value,
        timeout_ms: u64,
    ) -> Result<Option<RegistrationProof>> {
        let Some((verification, verification_sha256)) =
            self.observe_run(&["run".into(), "verify".into(), self.db(), run_id.into()])?
        else {
            return Ok(None);
        };
        let revision = verification["revision"]
            .as_u64()
            .context("owner revision missing")?;
        let state_digest = verification["state_digest"]
            .as_str()
            .context("owner digest missing")?;
        ensure!(
            verification["run_id"] == run_id,
            "owner verification run mismatch"
        );
        let (snapshot, history_sha256) = self
            .observe_run(&[
                "run".into(),
                "history-at".into(),
                self.db(),
                run_id.into(),
                revision.to_string(),
            ])?
            .context("verified owner history is missing")?;
        ensure!(
            snapshot["schema_version"] == 1
                && snapshot["run_id"] == run_id
                && snapshot["revision"] == revision,
            "owner history identity mismatch"
        );
        ensure!(
            format!("sha256:{}", hex(&serde_json::to_vec(&snapshot)?)) == state_digest,
            "owner historical snapshot digest mismatch"
        );
        let state =
            WorkflowState::from_value(&snapshot["status"]).context("unknown owner state")?;
        ensure!(state != WorkflowState::Unknown, "unknown owner state");
        let bundle = task_bundle(
            &[TaskSpec {
                id: "task".into(),
                payload: payload.clone(),
                write,
                timeout_ms,
            }],
            false,
        )?;
        let checked = self.check_bundle(&bundle)?;
        ensure!(checked["ok"] == true, "kernel check lacks success envelope");
        let bundle_digest = checked["bundle_digest"]
            .as_str()
            .context("kernel bundle digest missing")?;
        let definitions = checked["workflows"]
            .as_array()
            .context("kernel definitions missing")?;
        ensure!(definitions.len() == 1, "unexpected owner definition count");
        let definition = &definitions[0];
        ensure!(
            definition["id"] == bundle["root"]["id"]
                && definition["version"] == bundle["root"]["version"],
            "kernel root mismatch"
        );
        let frames = snapshot["frames"]
            .as_object()
            .context("owner frames missing")?;
        ensure!(frames.len() == 1, "unexpected owner frame count");
        let frame = frames.get("1").context("owner root frame missing")?;
        ensure!(
            snapshot["bundle_digest"] == bundle_digest
                && frame["workflow"] == bundle["root"]
                && definition["digest"].is_string()
                && frame["definition_digest"] == definition["digest"],
            "owner bundle or frame binding mismatch"
        );
        WorkflowFrameState::from_value(&frame["status"]).context("unknown owner frame state")?;
        let nodes = frame["nodes"].as_object().context("owner nodes missing")?;
        ensure!(
            nodes.len() == 2 && nodes.contains_key("task") && nodes.contains_key("done"),
            "owner node structure mismatch"
        );
        for node in nodes.values() {
            WorkflowNodeState::from_value(&node["state"]["state"])
                .context("unknown owner node state")?;
        }
        let observed: Value = serde_json::from_str(
            frame["nodes"]["task"]["inputs"]["task"]
                .as_str()
                .context("owner task input missing")?,
        )?;
        ensure!(observed == payload, "owner task input mismatch");
        Ok(Some(RegistrationProof {
            run_id: run_id.into(),
            revision,
            state,
            state_digest: state_digest.into(),
            bundle_digest: bundle_digest.into(),
            verification_sha256,
            history_sha256,
            kernel_result_sha256: hex(&serde_json::to_vec(&checked)?),
        }))
    }
}
