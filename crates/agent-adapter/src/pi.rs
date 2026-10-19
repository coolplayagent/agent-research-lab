use super::*;
use contracts::{PiEventKind as Event, PiStopReason as Stop};
pub(super) fn execute(options: &Options, prompt: &str, request: &str) -> Result<Value> {
    let mut args = vec!["--mode".into(), "rpc".into(), "--no-session".into()];
    model(options, &mut args);
    let mut child = session(options, &args)?;
    send(
        &mut child,
        json!({"id":request,"type":"prompt","message":prompt}),
    )?;
    let mut accepted = false;
    let mut settled = false;
    let mut response = None;
    while let Some(line) = child.next()? {
        let record: Value = serde_json::from_str(&line).context("invalid Pi protocol record")?;
        let event = Event::from_value(&record["type"]).context("unsupported Pi event")?;
        match event {
            Event::Response => {
                ensure!(
                    record["id"] == request
                        && record["command"] == "prompt"
                        && record["success"] == true,
                    "Pi prompt was rejected"
                );
                accepted = true;
            }
            Event::MessageEnd if record["message"]["role"] == "assistant" => {
                let reason = Stop::from_value(&record["message"]["stopReason"])
                    .context("unknown Pi stop reason")?;
                match reason {
                    Stop::Stop => response = Some(text_blocks(&record["message"]["content"])?),
                    Stop::ToolUse => response = None,
                    _ => bail!("Pi did not complete the task successfully"),
                }
            }
            Event::AgentSettled => {
                settled = true;
                child.close_input();
            }
            Event::AgentStart if settled => bail!("Pi started another run after settlement"),
            _ => {}
        }
    }
    child.wait()?;
    ensure!(
        accepted && settled,
        "Pi exited before the accepted task settled"
    );
    Ok(serde_json::from_str(
        &response.context("Pi final assistant result is missing")?,
    )?)
}
