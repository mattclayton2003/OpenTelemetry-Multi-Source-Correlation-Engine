use correlation_core::{Engine, CorrelationConfig, backend_mock::MockBackend, time::TestClock};
use std::{path::PathBuf, sync::Arc};
use chrono::Utc;

async fn engine_for(name: &str) -> Engine {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("tests/fixtures/edge_cases/{name}"));
    let b = Arc::new(MockBackend::from_fixture_dir(dir).unwrap());
    Engine::new(b, CorrelationConfig::default(), Arc::new(TestClock { now: Utc::now() }))
}

#[tokio::test]
async fn trace_not_found_returns_empty_with_note() {
    let e = engine_for("trace_not_found").await;
    let ic = e.correlate_trace("does-not-exist".into()).await.unwrap();
    assert!(ic.suspects.is_empty());
    assert!(ic.notes.iter().any(|n| n.to_lowercase().contains("not found")));
}

#[tokio::test]
async fn empty_window_returns_empty_with_note() {
    let e = engine_for("empty_window").await;
    let ic = e.correlate_trace("nothing".into()).await.unwrap();
    assert!(ic.suspects.is_empty());
    assert!(!ic.notes.is_empty());
}
