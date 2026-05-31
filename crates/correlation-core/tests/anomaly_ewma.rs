use correlation_core::anomaly::{Detector, ewma::Ewma};
use correlation_core::backend::MetricPoint;
use chrono::{Utc, Duration};

fn pt(s: i64, v: f64) -> MetricPoint {
    MetricPoint { ts: Utc::now() + Duration::seconds(s), service: "svc".into(), value: v }
}

#[test]
fn ewma_flags_sustained_shift() {
    let mut s: Vec<_> = (0..30).map(|i| pt(i, 1.0)).collect();
    for i in 30..40 { s.push(pt(i, 10.0)); }
    let det = Ewma { alpha: 0.3, k: 3.0, min_baseline: 10 };
    assert!(!det.detect(&s).is_empty());
}
