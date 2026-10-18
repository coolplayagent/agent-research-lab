use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
#[derive(Parser)]
#[command(
    name = "outcome-hub-migrate",
    about = "Offline, copy-only OutcomeHub state migration"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    Inspect {
        #[arg(long)]
        source: PathBuf,
    },
    Convert {
        #[arg(long)]
        source: PathBuf,
        #[arg(long)]
        destination: PathBuf,
    },
    Verify {
        #[arg(long)]
        source: PathBuf,
        #[arg(long)]
        destination: PathBuf,
    },
}
fn main() -> Result<()> {
    let result = match Cli::parse().command {
        Command::Inspect { source } => {
            serde_json::to_value(outcome_hub_migrate::inspect(&source)?)?
        }
        Command::Convert {
            source,
            destination,
        } => serde_json::to_value(outcome_hub_migrate::convert(&source, &destination)?)?,
        Command::Verify {
            source,
            destination,
        } => serde_json::to_value(outcome_hub_migrate::verify(&source, &destination)?)?,
    };
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
