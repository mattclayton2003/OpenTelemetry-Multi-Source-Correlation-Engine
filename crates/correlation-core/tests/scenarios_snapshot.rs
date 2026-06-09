use chrono::{TimeZone, Utc};
use correlation_core::{backend_mock::MockBackend, time::TestClock, CorrelationConfig, Engine};
use std::{path::PathBuf, sync::Arc};

#[tokio::test]
async fn payment_storm_fixture_snapshot() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/scenarios/payment_storm_synthetic");
    let backend = Arc::new(MockBackend::from_fixture_dir(dir).unwrap());
    let fixed = Utc.with_ymd_and_hms(2026, 5, 23, 12, 0, 0).unwrap();
    let engine = Engine::new(
        backend,
        CorrelationConfig::default(),
        Arc::new(TestClock { now: fixed }),
    );
    let ic = engine
        .correlate_trace("trace-pay-storm".into())
        .await
        .unwrap();
    // incident_id is UUIDv7 — replace before snapshotting
    let mut redacted = serde_json::to_value(&ic).unwrap();
    redacted["incident_id"] = serde_json::Value::String("<redacted>".into());
    redacted["elapsed_ms"] = serde_json::Value::from(0);
    insta::assert_json_snapshot!(redacted);
}
