use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    version,
    about = "AI-IM multi-agent collaboration; research is a group task scenario"
)]
pub(crate) struct Cli {
    #[arg(long, global = true)]
    pub(crate) config: Option<PathBuf>,
    #[command(subcommand)]
    pub(crate) command: Commands,
}
#[derive(Subcommand)]
pub(crate) enum Commands {
    /// Exercise real durable TCP/SSE communication in a new private state directory.
    CrystalBench {
        #[arg(long)]
        state: PathBuf,
        #[arg(long, value_parser = ["mixed", "hot"])]
        mode: String,
        #[arg(long, default_value_t = 10000)]
        people: usize,
        #[arg(long, default_value_t = 10000)]
        messages: usize,
        #[arg(long, default_value_t = 1000)]
        rate: usize,
    },
    /// Print the host-only browser bootstrap link from masked controller state.
    ServeLink {
        #[arg(long)]
        state_dir: Option<PathBuf>,
    },
    /// Serve AI-IM groups and goals. --config enables the optional research scenario.
    Serve {
        #[arg(long)]
        state_dir: Option<PathBuf>,
        #[arg(long, default_value = "127.0.0.1:8090")]
        listen: std::net::SocketAddr,
        /// Bound this observer's lifetime independently from the research controller.
        #[arg(long, default_value_t = 43200, value_parser = clap::value_parser!(u64).range(1..=43200))]
        max_seconds: u64,
    },
    /// Inspect agent adapters and bounded team communication, or simulate logical scale.
    Agents {
        #[arg(value_parser = ["backends", "boards", "view", "close", "prune", "simulate"])]
        action: String,
        #[arg(long)]
        cohort: Option<String>,
        #[arg(long, default_value_t = 0)]
        shard: u8,
        #[arg(long)]
        after: Option<String>,
        #[arg(long, default_value_t = 32)]
        limit: usize,
        #[arg(long, default_value_t = 1000)]
        agent_count: usize,
    },
    /// Plan, enqueue or collect one bounded, evidence-bound collaboration pilot.
    Collaboration {
        #[arg(value_parser = ["plan", "enqueue", "collect"])]
        action: String,
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        adjudications: Option<PathBuf>,
    },
    /// Prepare and exercise disposable real local CLI targets.
    Targets {
        #[arg(value_parser = ["prepare", "status", "verify", "agent-check"])]
        action: String,
        #[arg(long, help = "Target selection for verify", default_value = "all", value_parser = ["desktop", "sandbox", "all"])]
        kind: String,
    },
    #[command(hide = true)]
    TargetWorker {
        #[arg(long)]
        dependencies: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        computer_cli: PathBuf,
        #[arg(long)]
        verify: bool,
        #[arg(last = true)]
        command: Vec<String>,
    },
    /// Check tools and knowledge. Model probes make bounded live calls.
    Doctor {
        #[arg(long)]
        probe_models: bool,
    },
    /// Register immutable task JSON and its source/knowledge snapshots.
    Enqueue {
        input: PathBuf,
    },
    Seed,
    /// Fetch current upstream baselines and verify installed latest skills before research.
    Refresh,
    /// Compute the canonical installed-skill tree digest for a host manifest.
    SkillDigest {
        path: PathBuf,
    },
    /// Execute queued jobs. A single controller owns admission and daily accounting.
    Run {
        #[arg(long)]
        continuous: bool,
        #[arg(long, default_value_t = 0)]
        max_seconds: u64,
        #[arg(long)]
        seed: bool,
        /// Private JSON array of already enqueued IDs; requires a bounded, non-seeded run.
        #[arg(long)]
        task_ids_file: Option<PathBuf>,
    },
    Status,
    Pause,
    Resume,
    Retry {
        id: String,
    },
    Report,
    Install {
        input: PathBuf,
    },
    Rollback {
        receipt: PathBuf,
    },
    Pr {
        input: PathBuf,
    },
    Merge {
        input: PathBuf,
    },
    KnowledgePrepare {
        input: PathBuf,
    },
    KnowledgePublish {
        input: PathBuf,
    },
    KnowledgeRefresh {
        input: PathBuf,
    },
    /// Register/evaluate immutable prompt candidates; never self-approve formal policy.
    Evolution {
        action: String,
        input: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    Automation {
        action: String,
        #[arg(long)]
        input: Option<PathBuf>,
        #[arg(long)]
        id: Option<String>,
    },
}
