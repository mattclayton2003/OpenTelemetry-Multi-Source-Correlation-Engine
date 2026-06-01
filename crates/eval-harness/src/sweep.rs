use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct SweepConfig {
    pub anomaly: SweepAnomaly,
    pub ranking: SweepRanking,
    pub window:  SweepWindow,
}
#[derive(Debug, Deserialize)] pub struct SweepAnomaly { pub z_score_k: Vec<f64>, pub ewma_alpha: Vec<f64> }
#[derive(Debug, Deserialize)] pub struct SweepRanking { pub causal_propagation_beta: Vec<f64> }
#[derive(Debug, Deserialize)] pub struct SweepWindow  { pub expansion_sec: Vec<i64> }

pub fn cells(s: &SweepConfig) -> Vec<correlation_core::CorrelationConfig> {
    let mut out = vec![];
    for &k in &s.anomaly.z_score_k {
        for &a in &s.anomaly.ewma_alpha {
            for &b in &s.ranking.causal_propagation_beta {
                for &w in &s.window.expansion_sec {
                    let mut c = correlation_core::CorrelationConfig::default();
                    c.anomaly_zscore_k = k; c.anomaly_ewma_alpha = a;
                    c.causal_propagation_beta = b; c.window_expansion_sec = w;
                    out.push(c);
                }
            }
        }
    }
    out
}
