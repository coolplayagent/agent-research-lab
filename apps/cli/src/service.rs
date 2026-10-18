//! Application composition and process entrypoint. Durable execution lives in runtime.
use crate::cli::{Cli, Commands};
use anyhow::Result;
use clap::Parser;
use config::Config;
use serde_json::{Value, json};

fn execute(cli: Cli) -> Result<Value> {
    if cli.config.is_some() {
        anyhow::ensure!(
            !matches!(
                &cli.command,
                Commands::Serve {
                    state_dir: Some(_),
                    ..
                } | Commands::ServeLink { state_dir: Some(_) }
            ),
            "--config uses its configured state_dir; do not also pass --state-dir"
        );
    }
    match cli.command {
        Commands::Knowledge { endpoint, command } => {
            crate::knowledge_cli::execute(&endpoint, command)
        }
        Commands::SoftwareActivate {
            input,
            policy_digest,
        } => {
            let config = cli.config.unwrap_or_else(|| "local.toml".into());
            crate::software_activation::request(&config, &input, &policy_digest)
        }
        Commands::SoftwareActivationReconcile { operation } => {
            crate::software_activation::reconcile(&operation)
        }
        Commands::SoftwareActivationWorker { operation } => {
            crate::software_activation::worker(&operation)
        }
        Commands::Serve {
            listen,
            max_seconds,
            state_dir,
            local_operator,
        } => {
            if let Some(path) = &cli.config {
                let c = Config::load(path)?;
                anyhow::ensure!(
                    state_dir.is_none(),
                    "--config uses its configured state_dir; do not also pass --state-dir"
                );
                crate::dashboard::serve(&c, listen, max_seconds, local_operator)
            } else {
                let state = state_dir.unwrap_or_else(|| ".ai-im".into());
                tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(4)
                    .enable_all()
                    .build()?
                    .block_on(collaboration_host::serve_with_local_operator(
                        &state,
                        listen,
                        max_seconds,
                        local_operator,
                    ))?;
                Ok(json!({"stopped":true,"mode":"ai-im"}))
            }
        }
        Commands::ServeLink { state_dir } => {
            let state = if let Some(path) = &cli.config {
                Config::load(path)?.state_dir
            } else {
                state_dir.unwrap_or_else(|| ".ai-im".into())
            };
            storage::read(&state.join("dashboard/access.json"))
        }
        Commands::CrystalBench {
            state,
            mode,
            people,
            messages,
            rate,
        } => crate::dashboard::benchmark::run(&state, &mode, people, messages, rate),
        Commands::Agents {
            action,
            agent_count,
            ..
        } if action == "simulate" => multi_agent::scale::simulate(agent_count),
        Commands::SkillDigest { path } => {
            Ok(json!({"sha256":freshness::skill_tree_digest(&path)?}))
        }
        Commands::Evolution {
            action,
            input,
            output,
        } => evolution_cli::execute(&action, &input, output.as_deref()),
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
            let c = Config::load(&cli.config.unwrap_or_else(|| "local.toml".into()))?;
            match command {
                Commands::Agents {
                    action,
                    cohort,
                    shard,
                    after,
                    limit,
                    agent_count,
                } => {
                    if action == "simulate" {
                        return multi_agent::scale::simulate(agent_count);
                    }
                    if action == "backends" {
                        let configured: Vec<_> = c.agent_backends.iter().map(|(id,spec)| json!({"id":id,"declared_capabilities":spec.capabilities(),"compatibility":"requires_adapter_specific_validation"})).collect();
                        return Ok(
                            json!({"default":"codex","role_backends":c.role_backends,"models":c.models,"configured":configured,"max_active_workers":c.max_agents}),
                        );
                    }
                    let board = communication::Board::new(&c.state_dir)?;
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
                    "prepare" => targets::prepare(&c),
                    "status" => targets::status(&c),
                    "verify" => targets::verify(&c, &kind),
                    "agent-check" => targets::agent_check(&c),
                    _ => unreachable!(),
                },
                Commands::Collaboration {
                    action,
                    input,
                    output,
                    adjudications,
                } => collaboration_experiment::execute(
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
                Commands::EnqueueBatch { input, max_seconds } => Ok(serde_json::to_value(
                    runtime::batch::enqueue(&c, &input, max_seconds)?,
                )?),
                Commands::Seed => runtime::seed(&c),
                Commands::Refresh => Ok(serde_json::to_value(inputs::collect(&c)?)?),
                Commands::Run {
                    continuous,
                    max_seconds,
                    seed,
                    task_ids_file,
                } => {
                    runtime::run_scoped(&c, continuous, max_seconds, seed, task_ids_file.as_deref())
                }
                Commands::Status { summary: true } => runtime::status_summary(&c),
                Commands::Status { summary: false } | Commands::Report => runtime::status(&c),
                Commands::Pause => runtime::pause(&c, true),
                Commands::Resume => runtime::pause(&c, false),
                Commands::Retry { id } => runtime::retry(&c, &id),
                Commands::Automation { action, input, id } => {
                    let outbox = c.state_dir.join("outbox");
                    match action.as_str() {
                        "enqueue" => Ok(serde_json::to_value(automation::enqueue(
                            &outbox,
                            &storage::read(
                                &input
                                    .ok_or_else(|| anyhow::anyhow!("enqueue requires --input"))?,
                            )?,
                        )?)?),
                        "tick" => Ok(serde_json::to_value(automation::tick(&outbox)?)?),
                        "reconcile-retry" => {
                            Ok(serde_json::to_value(automation::reconcile_retry(
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
        let result = targets::worker(dependencies, output, computer_cli, *verify, command);
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
            let mut envelope = json!({"ok":false,"error":format!("{e:#}")});
            if let Some(batch) = e.downcast_ref::<runtime::batch::Failure>() {
                envelope["code"] = json!(batch.code);
                envelope["registration"] = json!(batch.registration);
            }
            eprintln!("{envelope}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
#[path = "service_tests.rs"]
mod tests;
