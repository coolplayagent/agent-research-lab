use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
#[derive(Parser)]
#[command(about = "External Git and directory knowledge service")]
struct Cli {
    #[arg(long)]
    id: String,
    #[arg(long)]
    state_dir: PathBuf,
    #[arg(long)]
    socket: PathBuf,
    #[arg(long, default_value_t = 43200)]
    max_seconds: u64,
}
#[tokio::main(worker_threads = 4)]
async fn main() -> Result<()> {
    let c = Cli::parse();
    knowledge_service::serve(&c.id, &c.state_dir, &c.socket, c.max_seconds).await
}
