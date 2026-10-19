use crate::process::Session;
use anyhow::{Context, Result, bail, ensure};
use contracts::{ClaudeResultState, CodexEventKind, CodingAgent};
use serde_json::{Value, json};
use service_api::{AgentInput, AgentOutput};
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
    #[arg(long)]
    pub launcher_argument: Vec<String>,
    #[arg(long)]
    pub model: Option<String>,
    #[arg(long, default_value_t = 3600)]
    pub seconds: u64,
}
pub fn schema() -> Value {
    json!({"type":"object","additionalProperties":false,"required":["protocol_version","request_id","summary","succeeded","evidence"],"properties":{
        "protocol_version":{"type":"integer"},"request_id":{"type":"string"},"summary":{"type":"string"},"succeeded":{"type":"boolean"},"evidence":{"type":"object","additionalProperties":false,"properties":{},"required":[]}
    }})
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
    let prompt = format!(
        "Perform the following OutcomeHub task in the current disposable workspace. Return ONLY a JSON object matching this schema: {}. Copy protocol_version and request_id exactly from the request. Do not claim success unless the acceptance criteria are satisfied. Request:\n{}",
        schema(),
        serde_json::to_string(input)?
    );
    let value = match agent {
        CodingAgent::Codex => codex(options, &prompt)?,
        CodingAgent::ClaudeCode => claude(options, &prompt)?,
        CodingAgent::Pi => pi::execute(options, &prompt, &input.request_id)?,
        CodingAgent::DeepseekHarness => dsh::execute(options, &prompt, &input.request_id)?,
        CodingAgent::Custom => bail!("custom adapters use the JSON stdio protocol directly"),
    };
    let output: AgentOutput =
        serde_json::from_value(value).context("invalid agent response schema")?;
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
