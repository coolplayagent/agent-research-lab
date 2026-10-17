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
        #[arg(long, default_value = ".ai-im")]
        state_dir: PathBuf,
        #[arg(long, default_value = "127.0.0.1:4877")]
        listen: SocketAddr,
        #[arg(long, default_value_t = 43200)]
        max_seconds: u64,
        /// Allow browser management directly on this trusted loopback host.
        #[arg(long)]
        local_operator: bool,
    },
    Link {
        #[arg(long, default_value = ".ai-im")]
        state_dir: PathBuf,
    },
}
#[tokio::main(worker_threads = 4)]
async fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Serve {
            state_dir,
            listen,
            max_seconds,
            local_operator,
        } => {
            collaboration_host::serve_with_local_operator(
                &state_dir,
                listen,
                max_seconds,
                local_operator,
            )
            .await
        }
        Command::Link { state_dir } => {
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
