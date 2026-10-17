//! Host-operated external knowledge client. No research configuration is loaded.
use anyhow::Result;
use clap::Subcommand;
use knowledge_model::{Configuration, Request, Target};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
#[derive(Subcommand)]
pub enum Command {
    List,
    /// Replace configuration using its current revision as a compare-and-swap precondition.
    Save {
        input: PathBuf,
    },
    /// Add or edit one target from a backend-neutral JSON configuration.
    Set {
        name: String,
        input: PathBuf,
    },
    Enable {
        name: String,
        #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
        enabled: bool,
    },
    Remove {
        name: String,
    },
    Probe {
        target: String,
    },
    Read {
        target: String,
        path: PathBuf,
        #[arg(long)]
        revision: Option<String>,
    },
    Preview {
        target: String,
        outcome: PathBuf,
    },
    /// Submit an explicitly reviewed, accepted-outcome write record from the host.
    Publish {
        input: PathBuf,
    },
    Receipt {
        id: String,
        request_digest: String,
    },
}
pub fn execute(endpoint: &Path, command: Command) -> Result<Value> {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?
        .block_on(run(endpoint, command))
}
async fn run(endpoint: &Path, command: Command) -> Result<Value> {
    use service_api::OutcomeServiceRequest;
    let request = match command {
        Command::List => Request::List,
        Command::Save { input } => Request::Save {
            configuration: storage::read(&input)?,
        },
        Command::Set { name, input } => {
            let target: Target = storage::read(&input)?;
            let mut configuration: Configuration =
                serde_json::from_value(knowledge_client::request(endpoint, Request::List).await?)?;
            configuration.targets.insert(name, target);
            Request::Save { configuration }
        }
        Command::Enable { name, enabled } => {
            let mut configuration: Configuration =
                serde_json::from_value(knowledge_client::request(endpoint, Request::List).await?)?;
            configuration
                .targets
                .get_mut(&name)
                .ok_or_else(|| anyhow::anyhow!("knowledge_target_not_found"))?
                .enabled = enabled;
            Request::Save { configuration }
        }
        Command::Remove { name } => {
            let mut configuration: Configuration =
                serde_json::from_value(knowledge_client::request(endpoint, Request::List).await?)?;
            anyhow::ensure!(
                configuration.targets.remove(&name).is_some(),
                "knowledge_target_not_found"
            );
            Request::Save { configuration }
        }
        Command::Probe { target } => Request::Probe { target },
        Command::Read {
            target,
            path,
            revision,
        } => Request::Read {
            target,
            path,
            revision,
        },
        Command::Preview { target, outcome } => Request::Preview {
            target,
            outcome: Box::new(storage::read(&outcome)?),
        },
        Command::Publish { input } => {
            let request: OutcomeServiceRequest = storage::read(&input)?;
            anyhow::ensure!(
                matches!(request, OutcomeServiceRequest::Write { .. }),
                "knowledge_write_required"
            );
            return service_protocol::call(endpoint, &request, Duration::from_secs(600)).await;
        }
        Command::Receipt { id, request_digest } => {
            return service_protocol::call(
                endpoint,
                &OutcomeServiceRequest::LookupWrite { id, request_digest },
                Duration::from_secs(60),
            )
            .await;
        }
    };
    knowledge_client::request(endpoint, request).await
}
