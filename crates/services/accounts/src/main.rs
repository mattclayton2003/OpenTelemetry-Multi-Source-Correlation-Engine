use sqlx::postgres::PgPoolOptions;
use std::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _otel = bank_common::otel::init("accounts")?;
    let metrics = bank_common::metrics::MetricsState::new();
    let url = std::env::var("DATABASE_URL")?;
    // Bound the wait for a pooled connection so a stalled DB (the db-down /
    // partition chaos faults) makes requests fail fast with a 5xx instead of
    // hanging ~30s on the sqlx default — turning the fault into a detectable
    // error-rate spike rather than abandoned, never-exported spans.
    let pool = PgPoolOptions::new()
        .max_connections(8)
        .acquire_timeout(Duration::from_secs(3))
        .connect(&url)
        .await?;
    sqlx::migrate!("./migrations").run(&pool).await?;

    let app = axum::Router::new()
        .merge(accounts::routes::router(pool))
        .merge(bank_common::health::router())
        .merge(bank_common::metrics::router(metrics))
        .layer(axum::middleware::from_fn(
            bank_common::otel::propagate_trace_context,
        ));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8002").await?;
    tracing::info!("accounts listening on 8002");
    axum::serve(listener, app).await?;
    Ok(())
}
