use axum::{http::header, response::IntoResponse, routing::get, Router};
use prometheus::{Encoder, Registry, TextEncoder};
use std::sync::Arc;

#[derive(Clone)]
pub struct MetricsState {
    pub registry: Arc<Registry>,
}

impl Default for MetricsState {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricsState {
    pub fn new() -> Self {
        Self {
            registry: Arc::new(Registry::new()),
        }
    }
}

pub fn router(state: MetricsState) -> Router {
    Router::new().route(
        "/metrics",
        get(move || {
            let reg = state.registry.clone();
            async move {
                let mut buf = Vec::new();
                let encoder = TextEncoder::new();
                encoder.encode(&reg.gather(), &mut buf).ok();
                ([(header::CONTENT_TYPE, encoder.format_type())], buf).into_response()
            }
        }),
    )
}
