use crate::*;
use std::future::IntoFuture;
/// A bounded standalone host. A supervisor can restart it against the same durable state.
pub async fn serve(state: &Path, listen: SocketAddr, seconds: u64) -> Result<()> {
    serve_with_local_operator(state, listen, seconds, false).await
}
pub async fn serve_with_local_operator(
    state: &Path,
    listen: SocketAddr,
    seconds: u64,
    local_operator: bool,
) -> Result<()> {
    validate_listen(listen, local_operator)?;
    ensure!(
        (1..=43200).contains(&seconds),
        "service lifetime must be 1..43200 seconds"
    );
    std::fs::create_dir_all(state)?;
    let services = Arc::new(service_api::Registry::open(state)?);
    let hub = services.configuration().storage(state)?;
    hub.register_operator().await?;
    let listener = tokio::net::TcpListener::bind(listen).await?;
    let addr = listener.local_addr()?;
    let (shutdown, _) = tokio::sync::watch::channel(false);
    let mut web = transport::Web::new(hub, addr, shutdown.clone());
    web.services = Some(services.clone());
    let iterations = tokio::spawn(crate::iteration::run_with_registry(
        web.hub.clone(),
        web.scenarios.clone(),
        Some(services.clone()),
        seconds,
        shutdown.subscribe(),
    ));
    let execution = tokio::spawn(crate::execution::run(
        web.hub.clone(),
        services,
        seconds,
        shutdown.subscribe(),
    ));
    web.operator = Some(Arc::new(if local_operator {
        operator::Operator::local(state, addr)?
    } else {
        operator::Operator::open(state, addr)?
    }));
    eprintln!(
        "AI-IM: http://{addr}; access link: ai-im link --state-dir {}",
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
