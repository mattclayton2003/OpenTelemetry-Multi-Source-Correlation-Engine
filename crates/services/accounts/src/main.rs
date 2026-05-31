use sqlx::postgres::PgPoolOptions;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _otel = bank_common::otel::init("accounts")?;
    let metrics = bank_common::metrics::MetricsState::new();
    let url = std::env::var("DATABASE_URL")?;
    let pool = PgPoolOptions::new().max_connections(8).connect(&url).await?;
    sqlx::migrate!("./migrations").run(&pool).await?;

    let app = axum::Router::new()
        .merge(accounts::routes::router(pool))
        .merge(bank_common::health::router())
        .merge(bank_common::metrics::router(metrics));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8002").await?;
    tracing::info!("accounts listening on 8002");
    axum::serve(listener, app).await?;
    Ok(())
}
