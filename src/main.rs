use agent_research_lab::{config::Config, delivery, knowledge, runtime, storage};
use anyhow::Result;
use clap::{Parser, Subcommand};
use serde_json::{Value, json};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    version,
    about = "Evidence-driven AI-SDLC research; knowledge in SuperPOD"
)]
struct Cli {
    #[arg(long, global = true, default_value = "local.toml")]
    config: PathBuf,
    #[command(subcommand)]
    command: Commands,
}
#[derive(Subcommand)]
enum Commands {
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
    /// Execute queued jobs. A single controller owns admission and daily accounting.
    Run {
        #[arg(long)]
        continuous: bool,
        #[arg(long, default_value_t = 0)]
        max_seconds: u64,
        #[arg(long)]
        seed: bool,
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
fn execute(cli: Cli) -> Result<Value> {
    match cli.command {
        Commands::Evolution {
            action,
            input,
            output,
        } => agent_research_lab::evolution_cli::execute(&action, &input, output.as_deref()),
        Commands::Install { input } => Ok(serde_json::to_value(delivery::install_candidate(
            &storage::read(&input)?,
        )?)?),
        Commands::Rollback { receipt } => {
            Ok(serde_json::to_value(delivery::rollback_install(&receipt)?)?)
        }
        Commands::Pr { input } => Ok(serde_json::to_value(delivery::ensure_pull_request(
            &storage::read(&input)?,
        )?)?),
        Commands::Merge { input } => Ok(serde_json::to_value(delivery::merge_pull_request(
            &storage::read(&input)?,
        )?)?),
        Commands::KnowledgePrepare { input } => Ok(serde_json::to_value(
            knowledge::prepare_report(&storage::read(&input)?)?,
        )?),
        Commands::KnowledgePublish { input } => Ok(serde_json::to_value(
            knowledge::publish_report(&storage::read(&input)?)?,
        )?),
        Commands::KnowledgeRefresh { input } => Ok(serde_json::to_value(
            knowledge::refresh_index(&storage::read(&input)?)?,
        )?),
        command => {
            let c = Config::load(&cli.config)?;
            match command {
                Commands::Doctor { probe_models } => runtime::doctor(&c, probe_models),
                Commands::Enqueue { input } => Ok(serde_json::to_value(runtime::enqueue(
                    &c,
                    storage::read(&input)?,
                )?)?),
                Commands::Seed => runtime::seed(&c),
                Commands::Run {
                    continuous,
                    max_seconds,
                    seed,
                } => {
                    if seed {
                        runtime::seed(&c)?;
                    }
                    runtime::run(&c, continuous, max_seconds)
                }
                Commands::Status | Commands::Report => runtime::status(&c),
                Commands::Pause => runtime::pause(&c, true),
                Commands::Resume => runtime::pause(&c, false),
                Commands::Retry { id } => runtime::retry(&c, &id),
                Commands::Automation { action, input, id } => {
                    let outbox = c.state_dir.join("outbox");
                    match action.as_str() {
                        "enqueue" => {
                            Ok(serde_json::to_value(
                                agent_research_lab::automation::enqueue(
                                    &outbox,
                                    &storage::read(&input.ok_or_else(|| {
                                        anyhow::anyhow!("enqueue requires --input")
                                    })?)?,
                                )?,
                            )?)
                        }
                        "tick" => Ok(serde_json::to_value(agent_research_lab::automation::tick(
                            &outbox,
                        )?)?),
                        "reconcile-retry" => Ok(serde_json::to_value(
                            agent_research_lab::automation::reconcile_retry(
                                &outbox,
                                &id.ok_or_else(|| {
                                    anyhow::anyhow!("reconcile-retry requires --id")
                                })?,
                            )?,
                        )?),
                        _ => anyhow::bail!("unknown automation action"),
                    }
                }
                _ => unreachable!(),
            }
        }
    }
}
fn main() {
    match execute(Cli::parse()) {
        Ok(result) => println!(
            "{}",
            serde_json::to_string_pretty(&json!({"ok":true,"result":result})).unwrap()
        ),
        Err(e) => {
            eprintln!("{}", json!({"ok":false,"error":format!("{e:#}")}));
            std::process::exit(1);
        }
    }
}
