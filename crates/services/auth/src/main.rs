use auth::routes;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _otel = bank_common::otel::init("auth")?;
    let metrics = bank_common::metrics::MetricsState::new();

    let app = axum::Router::new()
        .merge(routes::router())
        .merge(bank_common::health::router())
        .merge(bank_common::metrics::router(metrics));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8001").await?;
    tracing::info!("auth listening on 8001");
    axum::serve(listener, app).await?;
    Ok(())
}
