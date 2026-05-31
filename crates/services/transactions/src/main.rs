#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _otel = bank_common::otel::init("transactions")?;
    let metrics = bank_common::metrics::MetricsState::new();
    let cfg = transactions::clients::Config::from_env();
    let app = axum::Router::new()
        .merge(transactions::routes::router(cfg))
        .merge(bank_common::health::router())
        .merge(bank_common::metrics::router(metrics));
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8003").await?;
    tracing::info!("transactions listening on 8003");
    axum::serve(listener, app).await?;
    Ok(())
}
