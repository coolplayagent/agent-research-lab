use anyhow::{Result, ensure};
use clap::Parser;
use std::io::Read;
fn main() {
    if let Err(error) = run() {
        eprintln!("agent bridge failed: {error}");
        std::process::exit(1);
    }
}
fn run() -> Result<()> {
    let options = agent_adapter::Options::parse();
    ensure!(
        std::env::var("OUTCOME_HUB_ISOLATED").as_deref() == Ok("1"),
        "agent bridge requires the OutcomeHub sandbox service"
    );
    let mut bytes = Vec::new();
    std::io::stdin()
        .take(256 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 256 * 1024, "agent input exceeds bound");
    let input: service_api::AgentInput = serde_json::from_slice(&bytes)?;
    let output = agent_adapter::execute(&options, &input)?;
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
