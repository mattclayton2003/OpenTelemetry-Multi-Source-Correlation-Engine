pub mod zscore;
pub mod ewma;
use crate::backend::MetricPoint;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone)]
pub struct AnomalyHit {
    pub ts: DateTime<Utc>, pub value: f64, pub baseline_mean: f64,
    pub baseline_stddev: f64, pub z_score: f64, pub detector: &'static str,
}

pub trait Detector {
    fn detect(&self, series: &[MetricPoint]) -> Vec<AnomalyHit>;
}
