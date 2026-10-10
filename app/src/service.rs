//! Application composition and process entrypoint. Durable execution lives in runtime.
use crate::{
    cli::{Cli, Commands},
    config::Config,
    delivery, knowledge, runtime, storage,
};
use anyhow::Result;
use clap::Parser;
use serde_json::{Value, json};

fn execute(cli: Cli) -> Result<Value> {
    match cli.command {
        Commands::Agents {
            action,
            agent_count,
            ..
        } if action == "simulate" => crate::multi_agent::scale::simulate(agent_count),
        Commands::SkillDigest { path } => {
            Ok(json!({"sha256":crate::freshness::skill_tree_digest(&path)?}))
        }
        Commands::Evolution {
            action,
            input,
            output,
        } => crate::evolution_cli::execute(&action, &input, output.as_deref()),
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
                Commands::Serve {
                    listen,
                    max_seconds,
                } => crate::dashboard::serve(&c, listen, max_seconds),
                Commands::Agents {
                    action,
                    cohort,
                    shard,
                    after,
                    limit,
                    agent_count,
                } => {
                    if action == "simulate" {
                        return crate::multi_agent::scale::simulate(agent_count);
                    }
                    if action == "backends" {
                        let configured: Vec<_> = c.agent_backends.iter().map(|(id,spec)| json!({"id":id,"declared_capabilities":spec.capabilities(),"compatibility":"requires_adapter_specific_validation"})).collect();
                        return Ok(
                            json!({"default":"codex","role_backends":c.role_backends,"models":c.models,"configured":configured,"max_active_workers":c.max_agents}),
                        );
                    }
                    let board = crate::communication::Board::new(&c.state_dir)?;
                    if action == "boards" {
                        return board.cohorts(shard, after.as_deref(), limit);
                    }
                    let id =
                        cohort.ok_or_else(|| anyhow::anyhow!("this action requires --cohort"))?;
                    let now = u64::try_from(chrono::Utc::now().timestamp())?;
                    match action.as_str() {
                        "view" => board.view(&id, now),
                        "close" => {
                            board.close_cohort(&id, now)?;
                            Ok(json!({"closed":id}))
                        }
                        "prune" => {
                            board.prune(&id, now)?;
                            Ok(json!({"pruned":id,"identity_retired":true}))
                        }
                        _ => unreachable!(),
                    }
                }
                Commands::Targets { action, kind } => match action.as_str() {
                    "prepare" => crate::targets::prepare(&c),
                    "status" => crate::targets::status(&c),
                    "verify" => crate::targets::verify(&c, &kind),
                    "agent-check" => crate::targets::agent_check(&c),
                    _ => unreachable!(),
                },
                Commands::Collaboration {
                    action,
                    input,
                    output,
                    adjudications,
                } => crate::collaboration_experiment::execute(
                    &c,
                    &action,
                    &input,
                    &output,
                    adjudications.as_deref(),
                ),
                Commands::Doctor { probe_models } => runtime::doctor(&c, probe_models),
                Commands::Enqueue { input } => Ok(serde_json::to_value(runtime::enqueue(
                    &c,
                    storage::read(&input)?,
                )?)?),
                Commands::Seed => runtime::seed(&c),
                Commands::Refresh => Ok(serde_json::to_value(crate::inputs::collect(&c)?)?),
                Commands::Run {
                    continuous,
                    max_seconds,
                    seed,
                    task_ids_file,
                } => {
                    runtime::run_scoped(&c, continuous, max_seconds, seed, task_ids_file.as_deref())
                }
                Commands::Status | Commands::Report => runtime::status(&c),
                Commands::Pause => runtime::pause(&c, true),
                Commands::Resume => runtime::pause(&c, false),
                Commands::Retry { id } => runtime::retry(&c, &id),
                Commands::Automation { action, input, id } => {
                    let outbox = c.state_dir.join("outbox");
                    match action.as_str() {
                        "enqueue" => Ok(serde_json::to_value(crate::automation::enqueue(
                            &outbox,
                            &storage::read(
                                &input
                                    .ok_or_else(|| anyhow::anyhow!("enqueue requires --input"))?,
                            )?,
                        )?)?),
                        "tick" => Ok(serde_json::to_value(crate::automation::tick(&outbox)?)?),
                        "reconcile-retry" => {
                            Ok(serde_json::to_value(crate::automation::reconcile_retry(
                                &outbox,
                                &id.ok_or_else(|| {
                                    anyhow::anyhow!("reconcile-retry requires --id")
                                })?,
                            )?)?)
                        }
                        _ => anyhow::bail!("unknown automation action"),
                    }
                }
                _ => unreachable!(),
            }
        }
    }
}
pub fn run() {
    let cli = Cli::parse();
    if let Commands::TargetWorker {
        dependencies,
        output,
        computer_cli,
        verify,
        command,
    } = &cli.command
    {
        let result = crate::targets::worker(dependencies, output, computer_cli, *verify, command);
        match result {
            Ok(code) => std::process::exit(code),
            Err(e) => {
                eprintln!("{}", json!({"ok":false,"error":format!("{e:#}")}));
                std::process::exit(1);
            }
        }
    }
    match execute(cli) {
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
