use crate::process::Session;
use anyhow::{Context, Result, bail, ensure};
use contracts::{ClaudeResultState, CodexEventKind, CodingAgent};
use serde_json::{Value, json};
use service_api::{AGENT_EVIDENCE_MAX_BYTES, AGENT_SUMMARY_MAX_BYTES, AgentInput, AgentOutput};
use std::path::PathBuf;
#[path = "dsh.rs"]
mod dsh;
#[path = "pi.rs"]
mod pi;

#[derive(clap::Parser)]
pub struct Options {
    #[arg(long)]
    pub agent: String,
    #[arg(long)]
    pub program: PathBuf,
    #[arg(long)]
    pub digest: String,
    #[arg(long)]
    pub configuration_digest: Option<String>,
    #[arg(long, allow_hyphen_values = true)]
    pub launcher_argument: Vec<String>,
    #[arg(long)]
    pub model: Option<String>,
    #[arg(long, default_value_t = 3600)]
    pub seconds: u64,
}
pub fn schema() -> Value {
    json!({"type":"object","additionalProperties":false,"required":["protocol_version","request_id","summary","succeeded","evidence_json"],"properties":{
        "protocol_version":{"type":"integer"},"request_id":{"type":"string"},
        "summary":{"type":"string","description":format!("The substantive deliverable, at most {AGENT_SUMMARY_MAX_BYTES} UTF-8 bytes. Use concise prose or a compact table.")},
        "succeeded":{"type":"boolean"},
        "evidence_json":{"type":"string","description":format!("A compact JSON object encoded as a string. Its decoded value must be at most {AGENT_EVIDENCE_MAX_BYTES} UTF-8 bytes. Preserve all requested evidence fields, including complete nested coordinator proposals and artifact references. Use concise citations and limitations; do not duplicate the full deliverable from summary.")}
    }})
}
fn prompt(input: &AgentInput) -> Result<String> {
    Ok(format!(
        "Perform the following OutcomeHub task in the current disposable workspace. Return ONLY a JSON object matching this schema: {}. Encode the complete requested evidence object as a compact JSON string in evidence_json; never discard required fields. Copy protocol_version and request_id exactly from the request. Set succeeded only when this request's assigned work and acceptance criteria are satisfied. This reports your execution result; only the host can accept the overall goal. Output limits are UTF-8 bytes, not characters: summary <= {AGENT_SUMMARY_MAX_BYTES} bytes; the decoded evidence_json string <= {AGENT_EVIDENCE_MAX_BYTES} bytes. Put the concise substantive deliverable in summary and compact sources, limitations and required structured fields in evidence_json. Avoid duplicate prose, unnecessary whitespace and unnecessary Unicode escapes. Preserve complete coordinator proposals when requested. Leave room below the limits; use tools to check UTF-8 byte lengths before submitting. If the assignment cannot fit, report that limitation rather than truncate required evidence or claim completion. Request:\n{}",
        schema(),
        serde_json::to_string(input)?
    ))
}
pub fn execute(options: &Options, input: &AgentInput) -> Result<AgentOutput> {
    ensure!(
        input.protocol_version == 1 && (1..=3600).contains(&options.seconds),
        "unsupported agent request"
    );
    ensure!(
        crate::executable_digest(&options.program, &options.launcher_argument)? == options.digest,
        "agent installation changed; validate the binding again"
    );
    if let Some(expected) = &options.configuration_digest {
        ensure!(
            std::env::var("OUTCOME_HUB_CONFIG_DIGEST").ok().as_ref() == Some(expected),
            "agent configuration changed; validate the binding again"
        );
    }
    let agent = CodingAgent::parse(&options.agent)?;
    let prompt = prompt(input)?;
    let value = match agent {
        CodingAgent::Codex => codex(options, &prompt)?,
        CodingAgent::ClaudeCode => claude(options, &prompt)?,
        CodingAgent::Pi => pi::execute(options, &prompt, &input.request_id)?,
        CodingAgent::DeepseekHarness => dsh::execute(options, &prompt, &input.request_id)?,
        CodingAgent::Custom => bail!("custom adapters use the JSON stdio protocol directly"),
    };
    decode(input, value)
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeOutput {
    protocol_version: u32,
    request_id: String,
    summary: String,
    succeeded: bool,
    evidence_json: String,
}
fn decode(input: &AgentInput, value: Value) -> Result<AgentOutput> {
    let native: NativeOutput =
        serde_json::from_value(value).context("invalid agent response schema")?;
    ensure!(
        native.evidence_json.len() <= AGENT_EVIDENCE_MAX_BYTES,
        "agent evidence exceeds bound ({} UTF-8 bytes; maximum {AGENT_EVIDENCE_MAX_BYTES})",
        native.evidence_json.len()
    );
    let evidence: Value =
        serde_json::from_str(&native.evidence_json).context("invalid agent evidence")?;
    ensure!(evidence.is_object(), "agent evidence must be an object");
    let output = AgentOutput {
        protocol_version: native.protocol_version,
        request_id: native.request_id,
        summary: native.summary,
        succeeded: native.succeeded,
        evidence,
    };
    output.validate(input)?;
    Ok(output)
}
fn session(options: &Options, args: &[String]) -> Result<Session> {
    let mut arguments = options.launcher_argument.clone();
    arguments.extend_from_slice(args);
    Session::spawn(
        &options.program,
        &arguments,
        &std::env::current_dir()?,
        &[],
        options.seconds,
        true,
    )
}
fn model(options: &Options, args: &mut Vec<String>) {
    if let Some(model) = &options.model {
        args.extend(["--model".into(), model.clone()]);
    }
}
fn codex(options: &Options, prompt: &str) -> Result<Value> {
    let files = tempfile::Builder::new()
        .prefix("protocol-")
        .tempdir_in(std::env::current_dir()?)?;
    let schema_path = files.path().join("schema.json");
    let result_path = files.path().join("result.json");
    std::fs::write(&schema_path, serde_json::to_vec(&schema())?)?;
    // The enclosing service owns OS isolation; nested vendor sandboxes must not
    // request desktop approval or relax the outer AppContainer/bubblewrap boundary.
    let mut args = vec![
        "exec".into(),
        "--json".into(),
        "--ephemeral".into(),
        "--skip-git-repo-check".into(),
        "--dangerously-bypass-approvals-and-sandbox".into(),
        "--output-schema".into(),
        schema_path.to_string_lossy().into_owned(),
        "--output-last-message".into(),
        result_path.to_string_lossy().into_owned(),
    ];
    model(options, &mut args);
    args.push("-".into());
    let mut child = session(options, &args)?;
    child.send(prompt.as_bytes())?;
    child.close_input();
    let mut completed = false;
    while let Some(line) = child.next()? {
        let value: Value = serde_json::from_str(&line).context("invalid Codex protocol record")?;
        let event =
            CodexEventKind::from_value(&value["type"]).context("unsupported Codex event")?;
        match event {
            CodexEventKind::TurnCompleted => {
                ensure!(!completed, "duplicate Codex completion");
                completed = true;
            }
            CodexEventKind::Error | CodexEventKind::TurnFailed => {
                bail!("Codex task failed; check its authentication and selected model")
            }
            _ => {}
        }
    }
    child.wait()?;
    ensure!(completed, "Codex exited without a completed turn");
    ensure!(
        std::fs::metadata(&result_path)?.len() <= 32 * 1024,
        "Codex response exceeds bound"
    );
    Ok(serde_json::from_slice(&std::fs::read(result_path)?)?)
}
fn claude(options: &Options, prompt: &str) -> Result<Value> {
    let mut args = vec![
        "-p".into(),
        "--output-format".into(),
        "json".into(),
        "--json-schema".into(),
        schema().to_string(),
        "--no-session-persistence".into(),
        "--permission-mode".into(),
        "bypassPermissions".into(),
        "--allowedTools".into(),
        "Read,Write,Edit,Bash".into(),
    ];
    model(options, &mut args);
    let mut child = session(options, &args)?;
    child.send(prompt.as_bytes())?;
    child.close_input();
    let mut bytes = String::new();
    while let Some(line) = child.next()? {
        ensure!(
            bytes.len() + line.len() <= 256 * 1024,
            "Claude response exceeds bound"
        );
        bytes.push_str(&line);
    }
    child.wait()?;
    let value: Value = serde_json::from_str(&bytes).context("invalid Claude response")?;
    validate_claude(&value)
}
fn validate_claude(value: &Value) -> Result<Value> {
    ensure!(
        value["type"] == "result"
            && value["is_error"] == false
            && ClaudeResultState::from_value(&value["subtype"]) == Some(ClaudeResultState::Success),
        "Claude did not report a successful result"
    );
    ensure!(
        value["structured_output"].is_object(),
        "Claude structured result is missing"
    );
    Ok(value["structured_output"].clone())
}
fn send(child: &mut Session, value: Value) -> Result<()> {
    let mut bytes = serde_json::to_vec(&value)?;
    bytes.push(b'\n');
    child.send(&bytes)
}
fn text_blocks(value: &Value) -> Result<String> {
    let blocks = value.as_array().context("assistant content is missing")?;
    let text = blocks
        .iter()
        .filter(|v| v["type"] == "text")
        .filter_map(|v| v["text"].as_str())
        .collect::<String>();
    ensure!(text.len() <= 32 * 1024, "assistant response exceeds bound");
    Ok(text)
}
#[cfg(test)]
#[path = "providers_tests.rs"]
mod tests;
