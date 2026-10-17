use super::*;
use contracts::BatchPromptSelection;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;

pub(super) const REQUEST_LIMIT: usize = 1024 * 1024;
pub(super) const PLAN_LIMIT: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema_version: u32,
    pub batch_id: String,
    pub tasks: Vec<Task>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PromptBinding {
    pub selection: BatchPromptSelection,
    pub role: String,
    pub version: Option<String>,
    pub template_sha256: String,
    pub rendered_task_sha256: String,
}

impl PromptBinding {
    pub fn capture(c: &Config, task: &Task) -> Result<Self> {
        Ok(Self {
            selection: if task.prompt_version.is_some() {
                BatchPromptSelection::Registered
            } else {
                BatchPromptSelection::Builtin
            },
            role: task.role.clone(),
            version: task.prompt_version.clone(),
            template_sha256: storage::digest(role_prompt(c, task)?.as_bytes()),
            rendered_task_sha256: storage::digest(rendered_task_prompt(c, task)?.as_bytes()),
        })
    }
    pub fn verify(&self, c: &Config, task: &Task) -> Result<()> {
        ensure!(
            digest(self)? == digest(&Self::capture(c, task)?)?,
            "frozen prompt binding changed"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Entry {
    pub prepared: registration::Prepared,
    pub prompt: PromptBinding,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Plan {
    pub schema_version: u32,
    pub request: Request,
    pub request_digest: String,
    pub original_request_sha256: String,
    pub config_digest: String,
    pub inputs: inputs::ExecutionInputs,
    pub input_identity: String,
    pub entries: Vec<Entry>,
}

pub(super) fn digest(value: &impl Serialize) -> Result<String> {
    Ok(storage::digest(&serde_json::to_vec(value)?))
}

pub(super) fn read_request(path: &Path) -> Result<(Request, String)> {
    use std::io::Read;
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    ensure!(file.metadata()?.is_file(), "request must be a regular file");
    let mut bytes = Vec::new();
    file.take((REQUEST_LIMIT + 1) as u64)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= REQUEST_LIMIT, "request exceeds 1 MiB");
    Ok((serde_json::from_slice(&bytes)?, storage::digest(&bytes)))
}

/// Publish complete bytes without replacing an existing authority file.
pub(super) fn publish(path: &Path, plan: &Plan) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(plan)?;
    ensure!(bytes.len() <= PLAN_LIMIT, "retained plan exceeds 16 MiB");
    let temporary = path.with_extension(format!(
        "prepare-{}-{}",
        std::process::id(),
        workflow::now_ms()?
    ));
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&temporary)?;
    let result = (|| {
        file.write_all(&bytes)?;
        file.sync_all()?;
        fs::hard_link(&temporary, path)?;
        fs::File::open(path.parent().context("plan parent missing")?)?.sync_all()?;
        Ok(())
    })();
    let cleanup = fs::remove_file(&temporary);
    result.and_then(|()| Ok(cleanup?))
}
