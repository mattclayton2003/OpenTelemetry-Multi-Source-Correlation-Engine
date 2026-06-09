use chrono::{Duration, Utc};
use correlation_core::anomaly::{zscore::ZScore, Detector};
use correlation_core::backend::MetricPoint;

fn pt(secs: i64, v: f64) -> MetricPoint {
    MetricPoint {
        ts: Utc::now() + Duration::seconds(secs),
        service: "svc".into(),
        value: v,
    }
}

#[test]
fn flags_clear_spike() {
    let mut series: Vec<_> = (0..30).map(|i| pt(i, 1.0)).collect();
    series.push(pt(31, 100.0));
    let det = ZScore {
        k: 3.0,
        min_baseline: 10,
    };
    let anoms = det.detect(&series);
    assert_eq!(anoms.len(), 1);
}

#[test]
fn no_flags_on_clean_baseline() {
    let series: Vec<_> = (0..30).map(|i| pt(i, 1.0)).collect();
    let det = ZScore {
        k: 3.0,
        min_baseline: 10,
    };
    assert!(det.detect(&series).is_empty());
}

#[test]
fn baseline_too_short_returns_empty() {
    let series: Vec<_> = (0..5).map(|i| pt(i, 1.0)).collect();
    let det = ZScore {
        k: 3.0,
        min_baseline: 10,
    };
    assert!(det.detect(&series).is_empty());
}

#[test]
fn zero_variance_treats_any_change_as_anomaly() {
    let mut series: Vec<_> = (0..30).map(|i| pt(i, 5.0)).collect();
    series.push(pt(31, 5.0001));
    let det = ZScore {
        k: 3.0,
        min_baseline: 10,
    };
    assert_eq!(det.detect(&series).len(), 1);
}
