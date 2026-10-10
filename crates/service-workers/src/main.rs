use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
#[derive(Parser)]
#[command(about = "Independent coding-agent executor or lightweight sandbox service")]
struct Cli {
    #[arg(long,value_parser=["executor","sandbox"])]
    kind: String,
    #[arg(long)]
    state_dir: PathBuf,
    #[arg(long)]
    socket: PathBuf,
    #[arg(long, default_value_t = 43200)]
    max_seconds: u64,
    #[arg(long, default_value_t = 4)]
    capacity: usize,
}
#[tokio::main(worker_threads = 4)]
async fn main() -> Result<()> {
    let args = Cli::parse();
    service_workers::serve(
        contracts::ServiceKind::parse(&args.kind)?,
        &args.state_dir,
        &args.socket,
        args.max_seconds,
        args.capacity,
    )
    .await
}
