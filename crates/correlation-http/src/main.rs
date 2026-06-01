use axum::{routing::{get, post}, Router, Json, extract::State, http::StatusCode};
use correlation_core::{Engine, CorrelationConfig, MultiBackend, IncidentContext};
use correlation_core::time::WallClock;
use serde::Deserialize;
use std::sync::Arc;

#[derive(Clone)]
struct Ctx { engine: Arc<Engine> }

#[derive(Deserialize)] struct TraceReq { trace_id: String }
#[derive(Deserialize)] struct AnomalyReq {
    metric: String, service: String,
    start: chrono::DateTime<chrono::Utc>, end: chrono::DateTime<chrono::Utc>,
    value: f64,
}

async fn correlate_trace(State(ctx): State<Ctx>, Json(req): Json<TraceReq>)
    -> Result<Json<IncidentContext>, (StatusCode, String)> {
    ctx.engine.correlate_trace(req.trace_id).await
        .map(Json).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}
async fn correlate_anomaly(State(ctx): State<Ctx>, Json(req): Json<AnomalyReq>)
    -> Result<Json<IncidentContext>, (StatusCode, String)> {
    ctx.engine.correlate_anomaly(req.metric, req.service, req.start, req.end, req.value).await
        .map(Json).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let backend = MultiBackend {
        traces:  Arc::new(correlation_tempo::TempoClient::new(std::env::var("TEMPO_URL").unwrap_or("http://tempo:3200".into()))),
        logs:    Arc::new(correlation_loki::LokiClient::new(std::env::var("LOKI_URL").unwrap_or("http://loki:3100".into()))),
        metrics: Arc::new(correlation_prom::PromClient::new(std::env::var("PROM_URL").unwrap_or("http://prometheus:9090".into()))),
    };
    let engine = Arc::new(Engine::new(Arc::new(backend), CorrelationConfig::default(), Arc::new(WallClock)));
    let app = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/correlate/trace",   post(correlate_trace))
        .route("/correlate/anomaly", post(correlate_anomaly))
        .with_state(Ctx { engine });
    let bind = std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8500".into());
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    println!("corr-http listening on {bind}");
    axum::serve(listener, app).await?;
    Ok(())
}
