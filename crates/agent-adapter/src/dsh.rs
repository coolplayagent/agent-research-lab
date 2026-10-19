use super::*;
use contracts::DshTurnReason;
pub(super) fn execute(options: &Options, prompt: &str, request: &str) -> Result<Value> {
    let route = options.model.clone().or_else(|| std::env::var("OUTCOME_HUB_DSH_MODEL").ok()).context("DeepSeek Harness requires its configured provider/model route; select it explicitly if automatic resolution is unavailable")?;
    let (provider, model) = route
        .split_once('/')
        .filter(|(p, m)| !p.is_empty() && !m.is_empty())
        .context("DeepSeek Harness model must be provider/model")?;
    let mut child = session(options, &["--profile".into(), "sdk".into()])?;
    send(
        &mut child,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"cwd":std::env::current_dir()?,"provider":provider,"model":model}}),
    )?;
    let mut initialized = false;
    let mut queued = false;
    let mut settled = false;
    let mut shutdown = false;
    let mut response = None;
    let mut reason = None;
    while let Some(line) = child.next()? {
        let record: Value =
            serde_json::from_str(&line).context("invalid DeepSeek Harness protocol record")?;
        ensure!(
            record["jsonrpc"] == "2.0" && record.get("error").is_none(),
            "DeepSeek Harness rejected the request"
        );
        if let Some(id) = record["id"].as_u64() {
            match id {
                1 => {
                    ensure!(
                        !initialized
                            && record["result"]["serverInfo"]["name"]
                                == "deepseek-harness-sdk-runtime",
                        "DeepSeek Harness handshake mismatch"
                    );
                    initialized = true;
                    send(
                        &mut child,
                        json!({"jsonrpc":"2.0","id":2,"method":"session/prompt","params":{"sessionId":request,"contentBlocks":[{"type":"text","text":prompt}]}}),
                    )?;
                }
                2 => {
                    ensure!(
                        initialized
                            && !queued
                            && record["result"]["messageId"]
                                .as_str()
                                .is_some_and(|s| !s.is_empty()),
                        "DeepSeek Harness enqueue receipt missing"
                    );
                    queued = true;
                    send(
                        &mut child,
                        json!({"jsonrpc":"2.0","id":3,"method":"session/wait","params":{"sessionId":request}}),
                    )?;
                }
                3 => {
                    ensure!(
                        queued && !settled && reason == Some(DshTurnReason::Completed),
                        "DeepSeek Harness task did not complete successfully"
                    );
                    settled = true;
                    send(
                        &mut child,
                        json!({"jsonrpc":"2.0","id":4,"method":"shutdown"}),
                    )?;
                }
                4 => {
                    ensure!(settled, "unexpected DeepSeek Harness shutdown");
                    shutdown = true;
                    child.close_input();
                }
                _ => bail!("unknown DeepSeek Harness response identity"),
            }
        } else if record["method"] == "session.event" && record["params"]["sessionId"] == request {
            let event = &record["params"]["event"];
            if event["type"] == "assistant/message" {
                response = Some(text_blocks(&event["data"]["message"]["content"])?);
            }
            if event["type"] == "turn/end" {
                reason = Some(
                    DshTurnReason::from_value(&event["data"]["reason"]["kind"])
                        .context("unknown DeepSeek Harness terminal state")?,
                );
            }
        }
    }
    child.wait()?;
    ensure!(
        settled && shutdown,
        "DeepSeek Harness exited before task settlement and shutdown"
    );
    Ok(serde_json::from_str(&response.context(
        "DeepSeek Harness assistant result is missing",
    )?)?)
}
