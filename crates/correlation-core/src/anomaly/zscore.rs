use super::{AnomalyHit, Detector};
use crate::backend::MetricPoint;

pub struct ZScore {
    pub k: f64,
    pub min_baseline: usize,
}

impl Detector for ZScore {
    /// Flags points that are anomalous relative to a *robust* baseline (median
    /// and MAD) computed over the whole window. Using the median/MAD instead of
    /// the previous "baseline = everything-but-the-last-point, observe only the
    /// last point" means a transient spike anywhere in the window is detected —
    /// not just one that happens to land on the final sample. This matters for
    /// batch evaluation of a fault that spikes then recovers inside the window.
    fn detect(&self, series: &[MetricPoint]) -> Vec<AnomalyHit> {
        if series.len() < self.min_baseline + 1 {
            return vec![];
        }
        let med = median(series.iter().map(|p| p.value));
        // Median absolute deviation — a spike inflates the mean/stddev but not
        // the median/MAD, so the baseline stays representative of normal.
        let mad = median(series.iter().map(|p| (p.value - med).abs()));
        let mut out = vec![];
        for p in series {
            let dev = (p.value - med).abs();
            let (z, flagged) = if mad > 0.0 {
                // 0.6745 scales MAD to be consistent with the stddev of a normal
                // distribution, so `k` keeps its usual "number of sigmas" meaning.
                let z = 0.6745 * dev / mad;
                (z, z > self.k)
            } else if dev > 0.0 {
                (f64::INFINITY, true)
            } else {
                (0.0, false)
            };
            if flagged {
                out.push(AnomalyHit {
                    ts: p.ts,
                    value: p.value,
                    baseline_mean: med,
                    baseline_stddev: mad,
                    z_score: z,
                    detector: "z_score",
                });
            }
        }
        out
    }
}

fn median(vals: impl Iterator<Item = f64>) -> f64 {
    let mut v: Vec<f64> = vals.collect();
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    }
}
