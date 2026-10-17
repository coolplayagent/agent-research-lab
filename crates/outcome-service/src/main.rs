use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
#[derive(Parser)]
#[command(about = "Optional outcome verifier or external Git knowledge publisher")]
struct Cli {
    #[arg(long)]
    config: PathBuf,
    #[arg(long)]
    state_dir: PathBuf,
    #[arg(long)]
    socket: PathBuf,
    #[arg(long, default_value_t = 43200)]
    max_seconds: u64,
}
#[tokio::main(worker_threads = 4)]
async fn main() -> Result<()> {
    let args = Cli::parse();
    outcome_service::serve(
        storage::read(&args.config)?,
        &args.state_dir,
        &args.socket,
        args.max_seconds,
    )
    .await
}
