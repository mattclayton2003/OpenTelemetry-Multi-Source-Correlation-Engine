use opentelemetry::global;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{runtime, trace as sdktrace, Resource};
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
