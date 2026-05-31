use super::{AnomalyHit, Detector};
use crate::backend::MetricPoint;

pub struct Ewma { pub alpha: f64, pub k: f64, pub min_baseline: usize }

impl Detector for Ewma {
    fn detect(&self, series: &[MetricPoint]) -> Vec<AnomalyHit> {
        if series.len() < self.min_baseline + 1 { return vec![]; }
        let mut ewma = series[0].value;
        let mut residuals: Vec<f64> = vec![];
        let mut out = vec![];
        for (i, p) in series.iter().enumerate() {
            let residual = p.value - ewma;
            if i >= self.min_baseline {
                let mean_r = residuals.iter().sum::<f64>() / residuals.len() as f64;
                let var_r  = residuals.iter().map(|r| (r - mean_r).powi(2)).sum::<f64>() / residuals.len() as f64;
                let sd_r   = var_r.sqrt();
                let z = if sd_r > 0.0 { (residual - mean_r).abs() / sd_r } else { 0.0 };
                if z > self.k {
                    out.push(AnomalyHit { ts: p.ts, value: p.value,
                        baseline_mean: ewma, baseline_stddev: sd_r,
                        z_score: z, detector: "ewma" });
                }
            }
            residuals.push(residual);
            ewma = self.alpha * p.value + (1.0 - self.alpha) * ewma;
        }
        out
    }
}
