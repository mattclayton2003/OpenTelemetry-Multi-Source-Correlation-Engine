use super::{AnomalyHit, Detector};
use crate::backend::MetricPoint;

pub struct ZScore { pub k: f64, pub min_baseline: usize }

impl Detector for ZScore {
    fn detect(&self, series: &[MetricPoint]) -> Vec<AnomalyHit> {
        if series.len() < self.min_baseline + 1 { return vec![]; }
        let split = series.len() - 1;
        let baseline = &series[..split];
        let mean = baseline.iter().map(|p| p.value).sum::<f64>() / baseline.len() as f64;
        let var = baseline.iter().map(|p| (p.value - mean).powi(2)).sum::<f64>() / baseline.len() as f64;
        let stddev = var.sqrt();
        let mut out = vec![];
        for p in &series[split..] {
            let z = if stddev > 0.0 { (p.value - mean).abs() / stddev } else if (p.value - mean).abs() > 0.0 { f64::INFINITY } else { 0.0 };
            let flagged = if stddev > 0.0 { z > self.k } else { (p.value - mean).abs() > 0.0 };
            if flagged {
                out.push(AnomalyHit { ts: p.ts, value: p.value, baseline_mean: mean,
                    baseline_stddev: stddev, z_score: z, detector: "z_score" });
            }
        }
        out
    }
}
