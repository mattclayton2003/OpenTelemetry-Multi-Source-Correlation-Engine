use opentelemetry::global;
use opentelemetry::propagation::{Extractor, Injector};
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use opentelemetry_sdk::{runtime, trace as sdktrace, Resource};
use tracing::Instrument;
use tracing_opentelemetry::OpenTelemetrySpanExt;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

pub struct OtelGuard;
impl Drop for OtelGuard {
    fn drop(&mut self) {
        opentelemetry::global::shutdown_tracer_provider();
    }
}

pub fn init(service_name: &'static str) -> anyhow::Result<OtelGuard> {
    let resource = Resource::new(vec![opentelemetry::KeyValue::new(
        "service.name",
        service_name,
    )]);

    let provider = match std::env::var("OTLP_ENDPOINT") {
        Ok(endpoint) => {
            // Async OTLP pipeline — requires Tokio runtime to be in scope.
            let exporter = opentelemetry_otlp::new_exporter()
                .tonic()
                .with_endpoint(endpoint);
            opentelemetry_otlp::new_pipeline()
                .tracing()
                .with_exporter(exporter)
                .with_trace_config(sdktrace::Config::default().with_resource(resource))
                .install_batch(runtime::Tokio)?
        }
        Err(_) => {
            // No endpoint — no-op fallback, no runtime needed.
            // Suitable for sync tests and early-main use before runtime spins up.
            sdktrace::TracerProvider::builder()
                .with_config(sdktrace::Config::default().with_resource(resource))
                .build()
        }
    };

    let tracer = provider.tracer(service_name);
    global::set_tracer_provider(provider);
    // Enable W3C trace-context propagation so a request's trace_id flows across
    // service boundaries (inject on outgoing HTTP, extract on incoming).
    global::set_text_map_propagator(TraceContextPropagator::new());

    let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);
    let fmt_layer = tracing_subscriber::fmt::layer().with_target(true);
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt_layer)
        .with(otel_layer)
        .try_init()
        .ok(); // ignore "already initialized" in tests

    Ok(OtelGuard)
}

// ---- W3C trace-context propagation across HTTP boundaries ----

/// Reads `traceparent`/`tracestate` from an incoming request's headers.
struct HeaderExtractor<'a>(&'a axum::http::HeaderMap);
impl Extractor for HeaderExtractor<'_> {
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).and_then(|v| v.to_str().ok())
    }
    fn keys(&self) -> Vec<&str> {
        self.0.keys().map(|k| k.as_str()).collect()
    }
}

/// Writes `traceparent`/`tracestate` onto an outgoing request's headers.
struct HeaderInjector<'a>(&'a mut axum::http::HeaderMap);
impl Injector for HeaderInjector<'_> {
    fn set(&mut self, key: &str, value: String) {
        if let (Ok(name), Ok(val)) = (
            axum::http::HeaderName::from_bytes(key.as_bytes()),
            axum::http::HeaderValue::from_str(&value),
        ) {
            self.0.insert(name, val);
        }
    }
}

/// Injects the current span's trace context into `headers`. Call this when
/// building an outgoing HTTP request so the downstream service joins this trace.
///
/// `reqwest::header::HeaderMap` and `axum::http::HeaderMap` are the same
/// `http` 1.x type, so the result can be passed straight to `RequestBuilder::headers`.
pub fn inject_current_context(headers: &mut axum::http::HeaderMap) {
    let cx = tracing::Span::current().context();
    global::get_text_map_propagator(|p| p.inject_context(&cx, &mut HeaderInjector(headers)));
}

/// Convenience: a fresh header map carrying the current trace context.
pub fn trace_headers() -> axum::http::HeaderMap {
    let mut headers = axum::http::HeaderMap::new();
    inject_current_context(&mut headers);
    headers
}

/// A CLIENT-kind span for an outgoing call to `peer_service`. Instrument the
/// request future with it (and call [`trace_headers`] inside) so the downstream
/// SERVER span links to it — the CLIENT→SERVER pairing is what populates
/// Zipkin's service dependency graph.
pub fn client_span(operation: &str, peer_service: &str) -> tracing::Span {
    tracing::info_span!(
        "client",
        otel.kind = "client",
        otel.name = operation,
        peer.service = peer_service,
    )
}

/// Axum middleware that extracts the incoming `traceparent` and runs the
/// request inside a server span parented to it, so every service's work for a
/// request shares one trace_id. Attach with
/// `.layer(axum::middleware::from_fn(bank_common::otel::propagate_trace_context))`.
pub async fn propagate_trace_context(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let parent = global::get_text_map_propagator(|p| p.extract(&HeaderExtractor(req.headers())));
    let span = tracing::info_span!(
        "http.server",
        otel.kind = "server",
        otel.name = %format!("{} {}", req.method(), req.uri().path()),
        http.method = %req.method(),
        http.path = %req.uri().path(),
    );
    span.set_parent(parent);
    next.run(req).instrument(span).await
}
