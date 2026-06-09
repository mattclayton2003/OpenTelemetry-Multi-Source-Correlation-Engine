use axum::{routing::get, Json, Router};
use serde::Serialize;

#[derive(Serialize)]
struct Status {
    status: &'static str,
}

pub fn router() -> Router {
    Router::new()
        .route("/health", get(|| async { Json(Status { status: "ok" }) }))
        .route("/ready", get(|| async { Json(Status { status: "ready" }) }))
}
