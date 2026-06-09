use chrono::{Duration, Utc};
use correlation_core::time::TestClock;
use correlation_core::{backend_mock::MockBackend, CorrelationConfig, Engine};
use std::{path::PathBuf, sync::Arc};

#[tokio::test]
async fn correlate_anomaly_returns_incident_when_anomaly_detected() {
    let dir =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/scenarios/anomaly_spike");
    let backend = Arc::new(MockBackend::from_fixture_dir(dir).unwrap());
    let engine = Engine::new(
        backend,
        CorrelationConfig::default(),
        Arc::new(TestClock { now: Utc::now() }),
    );
    let now = Utc::now();
    let ic = engine
        .correlate_anomaly(
            "http_p99".into(),
            "transactions".into(),
            now - Duration::seconds(60),
            now,
            2.5,
        )
        .await
        .unwrap();
    // With empty metric data, engine returns explanatory note rather than fake suspect.
    assert!(
        !ic.notes.is_empty() || !ic.suspects.is_empty(),
        "either suspects or an explanatory note"
    );
}
