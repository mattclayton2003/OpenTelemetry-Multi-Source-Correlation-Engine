use axum::{routing::get, Router, response::IntoResponse, http::header};
use prometheus::{Encoder, TextEncoder, Registry};
use std::sync::Arc;

#[derive(Clone)]
pub struct MetricsState { pub registry: Arc<Registry> }

impl MetricsState {
    pub fn new() -> Self { Self { registry: Arc::new(Registry::new()) } }
}

pub fn router(state: MetricsState) -> Router {
    Router::new().route("/metrics", get(move || {
        let reg = state.registry.clone();
        async move {
            let mut buf = Vec::new();
            let encoder = TextEncoder::new();
            encoder.encode(&reg.gather(), &mut buf).ok();
            ([(header::CONTENT_TYPE, encoder.format_type())], buf).into_response()
        }
    }))
}
