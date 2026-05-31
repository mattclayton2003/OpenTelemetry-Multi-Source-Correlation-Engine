use correlation_core::{Engine, CorrelationConfig, backend_mock::MockBackend};
use std::{path::PathBuf, sync::Arc};
use correlation_core::time::TestClock;
use chrono::Utc;

#[tokio::test]
async fn correlate_trace_emits_incident_with_suspects() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/scenarios/minimal");
    let backend = Arc::new(MockBackend::from_fixture_dir(dir).unwrap());
    let trace_id = backend.trace_by_id.keys().next().unwrap().clone();
    let engine = Engine::new(backend, CorrelationConfig::default(),
                              Arc::new(TestClock { now: Utc::now() }));
    let ic = engine.correlate_trace(trace_id).await.unwrap();
    assert_eq!(ic.schema_version, correlation_core::schema::SCHEMA_VERSION);
    assert!(!ic.spans.is_empty());
    assert!(ic.elapsed_ms < 5_000);
}
