#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _otel = bank_common::otel::init("notifications")?;
    let metrics = bank_common::metrics::MetricsState::new();
    let smtp_url = std::env::var("SMTP_URL").unwrap_or("http://smtp-fake:2525".into());
    let app = axum::Router::new()
        .merge(notifications::routes::router(smtp_url))
        .merge(bank_common::health::router())
        .merge(bank_common::metrics::router(metrics))
        .layer(axum::middleware::from_fn(
            bank_common::otel::propagate_trace_context,
        ));
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8004").await?;
    tracing::info!("notifications listening on 8004");
    axum::serve(listener, app).await?;
    Ok(())
}
