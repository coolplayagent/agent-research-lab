use crate::*;
use std::future::IntoFuture;
/// A bounded standalone host. A supervisor can restart it against the same durable state.
pub async fn serve(state: &Path, listen: SocketAddr, seconds: u64) -> Result<()> {
    ensure!(listen.ip().is_loopback(), "IM must bind a loopback address");
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
    let execution = tokio::spawn(crate::execution::run(
        web.hub.clone(),
        services,
        seconds,
        shutdown.subscribe(),
    ));
    web.operator = Some(Arc::new(operator::Operator::open(state, addr)?));
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
    let _ = execution.await;
    served??;
    Ok(())
}
