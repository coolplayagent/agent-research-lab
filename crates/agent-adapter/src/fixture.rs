//! Deterministic protocol peer for acceptance tests. Never shipped as an Agent.
use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::io::{BufRead, Read, Write};
fn emit(value: Value) {
    println!("{value}");
    std::io::stdout().flush().unwrap();
}
fn output(prompt: &str) -> Result<Value> {
    let request: Value = serde_json::from_str(
        prompt
            .split_once("Request:\n")
            .context("fixture request missing")?
            .1,
    )?;
    if std::path::Path::new("binding-input.txt").is_file() {
        std::fs::copy("binding-input.txt", "binding-result.txt")?;
    }
    Ok(
        json!({"protocol_version":1,"request_id":request["request_id"],"summary":"Deterministic fixture completed.","succeeded":true,"evidence":{}}),
    )
}
fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.iter().any(|a| a == "--version") {
        println!("OutcomeHub test fixture 1.0");
        return Ok(());
    }
    if args.iter().any(|a| a == "--dump-config") {
        println!("agents: [{{id: main, provider: fixture, model: deterministic}}]");
        return Ok(());
    }
    if std::env::current_exe()?
        .file_stem()
        .is_some_and(|name| name.to_string_lossy().contains("hang"))
    {
        std::thread::sleep(std::time::Duration::from_secs(180));
        return Ok(());
    }
    if args.iter().any(|a| a == "exec") {
        let mut prompt = String::new();
        std::io::stdin().read_to_string(&mut prompt)?;
        let result = output(&prompt)?;
        let path = &args[args
            .iter()
            .position(|a| a == "--output-last-message")
            .context("result path missing")?
            + 1];
        std::fs::write(path, result.to_string())?;
        emit(json!({"type":"thread.started","thread_id":"fixture"}));
        emit(json!({"type":"turn.started"}));
        emit(json!({"type":"turn.completed","usage":{}}));
    } else if args.iter().any(|a| a == "-p") {
        let mut prompt = String::new();
        std::io::stdin().read_to_string(&mut prompt)?;
        emit(
            json!({"type":"result","subtype":"success","is_error":false,"structured_output":output(&prompt)?}),
        );
    } else if args.iter().any(|a| a == "rpc") {
        for line in std::io::stdin().lock().lines() {
            let record: Value = serde_json::from_str(&line?)?;
            emit(
                json!({"id":record["id"],"type":"response","command":"prompt","success":true,"data":{"disposition":"started"}}),
            );
            emit(json!({"type":"agent_start"}));
            emit(
                json!({"type":"message_end","message":{"role":"assistant","stopReason":"stop","content":[{"type":"text","text":output(record["message"].as_str().unwrap())?.to_string()}]}}),
            );
            emit(json!({"type":"agent_end"}));
            emit(json!({"type":"agent_settled"}));
        }
    } else if args.iter().any(|a| a == "sdk") {
        let mut pending = None;
        for line in std::io::stdin().lock().lines() {
            let record: Value = serde_json::from_str(&line?)?;
            let result = match record["method"].as_str().unwrap_or_default() {
                "initialize" => {
                    json!({"serverInfo":{"name":"deepseek-harness-sdk-runtime","version":"0.0.1"}})
                }
                "session/prompt" => {
                    pending = Some(record["params"].clone());
                    json!({"messageId":"fixture-message"})
                }
                "session/wait" => {
                    let request = pending.take().context("fixture prompt missing")?;
                    let output = output(request["contentBlocks"][0]["text"].as_str().unwrap())?;
                    emit(
                        json!({"jsonrpc":"2.0","method":"session.event","params":{"sessionId":request["sessionId"],"event":{"type":"assistant/message","data":{"message":{"content":[{"type":"text","text":output.to_string()}]}}}}}),
                    );
                    emit(
                        json!({"jsonrpc":"2.0","method":"session.event","params":{"sessionId":request["sessionId"],"event":{"type":"turn/end","data":{"reason":{"kind":"completed"}}}}}),
                    );
                    json!({})
                }
                "shutdown" => {
                    emit(json!({"jsonrpc":"2.0","id":record["id"],"result":{}}));
                    break;
                }
                _ => anyhow::bail!("unexpected fixture method"),
            };
            emit(json!({"jsonrpc":"2.0","id":record["id"],"result":result}));
        }
    } else {
        anyhow::bail!("unsupported fixture invocation");
    }
    Ok(())
}
