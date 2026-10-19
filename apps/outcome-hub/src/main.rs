use anyhow::Result;
use clap::{Parser, Subcommand};
use std::{net::SocketAddr, path::PathBuf};
#[derive(Parser)]
#[command(
    name = "outcome-hub",
    about = "OutcomeHub: outcome-based multi-agent work system"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    Serve {
        #[arg(long)]
        state_dir: Option<PathBuf>,
        #[arg(long, default_value = "127.0.0.1:4877")]
        listen: SocketAddr,
        #[arg(long, default_value_t = 43200)]
        max_seconds: u64,
    },
    Link {
        #[arg(long)]
        state_dir: Option<PathBuf>,
    },
}
fn default_state_dir() -> Result<PathBuf> {
    #[cfg(windows)]
    {
        let root = std::env::var_os("LOCALAPPDATA")
            .ok_or_else(|| anyhow::anyhow!("LOCALAPPDATA is required for the default state"))?;
        Ok(PathBuf::from(root).join("OutcomeHub").join("state"))
    }
    #[cfg(not(windows))]
    {
        Ok(PathBuf::from(".ai-im"))
    }
}
#[tokio::main(worker_threads = 4)]
async fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Serve {
            state_dir,
            listen,
            max_seconds,
        } => {
            let state_dir = state_dir.map_or_else(default_state_dir, Ok)?;
            collaboration_host::serve(&state_dir, listen, max_seconds).await
        }
        Command::Link { state_dir } => {
            let state_dir = state_dir.map_or_else(default_state_dir, Ok)?;
            let value: serde_json::Value = storage::read(&state_dir.join("dashboard/access.json"))?;
            println!(
                "{}",
                value["url"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("start the service first"))?
            );
            Ok(())
        }
    }
}
