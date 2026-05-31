use bank_common::otel;

#[test]
fn init_does_not_panic_without_endpoint() {
    // OTLP_ENDPOINT unset → no-op fallback path, no Tokio runtime required.
    let _guard = otel::init("test-service").expect("init returns Ok");
}
