use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CorrelationConfig {
    pub window_expansion_sec: i64,
    pub log_bucket_sec: i64,
    pub anomaly_zscore_k: f64,
    pub anomaly_ewma_alpha: f64,
    pub causal_propagation_beta: f64,
    pub causal_propagation_max_depth: u8,
    pub min_baseline_sec: i64,
    /// A non-error span whose *self-time* (duration minus the time spent in its
    /// children) exceeds this many milliseconds is treated as latency evidence
    /// for its service. Self-time isolates the service actually doing the slow
    /// work from callers merely blocked waiting on a downstream dependency.
    pub slow_span_self_ms: i64,
}

impl Default for CorrelationConfig {
    fn default() -> Self {
        Self {
            window_expansion_sec: 30,
            log_bucket_sec: 10,
            anomaly_zscore_k: 3.0,
            anomaly_ewma_alpha: 0.3,
            causal_propagation_beta: 0.5,
            causal_propagation_max_depth: 3,
            min_baseline_sec: 60,
            slow_span_self_ms: 200,
        }
    }
}

impl CorrelationConfig {
    pub fn hash(&self) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let s = toml::to_string(self).unwrap_or_default();
        let mut h = DefaultHasher::new();
        s.hash(&mut h);
        format!("sha256:{:016x}", h.finish())
    }
}
