use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(about = "Optional Codex execution adapter for the AI-IM JSON stdio protocol")]
struct Cli {
    #[arg(long)]
    program: PathBuf,
    #[arg(long)]
    model: String,
    #[arg(long, default_value_t = 300)]
    max_seconds: u64,
}

fn main() -> Result<()> {
    let args = Cli::parse();
    let result =
        service_workers::codex_adapter::execute(&args.program, &args.model, args.max_seconds)?;
    serde_json::to_writer(std::io::stdout(), &result)?;
    Ok(())
}
