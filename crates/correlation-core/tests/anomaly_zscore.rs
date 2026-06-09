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
fn flags_transient_mid_window_spike() {
    // Spike in the MIDDLE of the window, normal again at the end. The previous
    // last-point-only detector missed this; the robust median/MAD detector
    // catches it regardless of where in the window it lands.
    let mut series: Vec<_> = (0..40).map(|i| pt(i, 5.0)).collect();
    for (i, s) in series.iter_mut().enumerate().take(20).skip(15) {
        *s = pt(i as i64, 900.0);
    }
    let det = ZScore {
        k: 3.0,
        min_baseline: 10,
    };
    let anoms = det.detect(&series);
    assert!(!anoms.is_empty(), "should detect a mid-window spike");
    assert!(
        anoms.iter().all(|a| a.value > 100.0),
        "only the spike points should be flagged, not the normal baseline"
    );
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
