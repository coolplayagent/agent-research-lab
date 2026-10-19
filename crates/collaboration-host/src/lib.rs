use anyhow::{Result, ensure};
use collaboration_api::{operator, transport, validate_listen};
use std::future::IntoFuture;
use std::{net::SocketAddr, path::Path, sync::Arc, time::Duration};
/// A bounded standalone host. A supervisor can restart it against the same durable state.
pub async fn serve(state: &Path, listen: SocketAddr, seconds: u64) -> Result<()> {
    validate_listen(listen)?;
    ensure!(
        (1..=43200).contains(&seconds),
        "service lifetime must be 1..43200 seconds"
    );
    storage::private_dir(state)?;
    let services = Arc::new(service_registry::Registry::open(state)?);
    let hub = storage(&services.configuration(), state)?;
    hub.register_operator().await?;
    let listener = tokio::net::TcpListener::bind(listen).await?;
    let addr = listener.local_addr()?;
    let (shutdown, _) = tokio::sync::watch::channel(false);
    let mut web = transport::Web::new(hub, addr, shutdown.clone());
    web.services = Some(services.clone());
    let iterations = tokio::spawn(collaboration_api::iteration::run_with_registry(
        web.hub.clone(),
        web.scenarios.clone(),
        Some(services.clone()),
        seconds,
        shutdown.subscribe(),
    ));
    let execution = tokio::spawn(collaboration_api::execution::run(
        web.hub.clone(),
        services,
        seconds,
        shutdown.subscribe(),
    ));
    web.operator = Some(Arc::new(operator::Operator::local(state, addr)?));
    eprintln!(
        "OutcomeHub workspace: http://{addr}; state: {}",
        state.display()
    );
    let halt = async move {
        tokio::time::sleep(Duration::from_secs(seconds)).await;
        shutdown.send_replace(true);
    };
    let served = tokio::time::timeout(
        Duration::from_secs(seconds + 5),
        axum::serve(listener, transport::router(web, axum::Router::new()))
            .with_graceful_shutdown(halt)
            .into_future(),
    )
    .await;
    execution.abort();
    iterations.abort();
    let _ = iterations.await;
    let _ = execution.await;
    served??;
    Ok(())
}

/// Bind remote production storage or an explicitly separate embedded development store.
pub fn storage(
    configuration: &service_api::Configuration,
    state: &Path,
) -> Result<collaboration_client::Client> {
    match &configuration.storage_id {
        Some(id) => collaboration_client::Client::remote(
            configuration
                .service(id, contracts::ServiceKind::Storage)?
                .endpoint
                .clone(),
        ),
        None => Ok(collaboration_storage_service::client(
            collaboration_store::Hub::open(&state.join("crystal"))?,
        )),
    }
}
