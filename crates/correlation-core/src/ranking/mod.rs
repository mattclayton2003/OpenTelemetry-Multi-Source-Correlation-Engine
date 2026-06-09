pub mod propagation;
pub mod scoring;

#[derive(Debug, Clone)]
pub struct ScoredSuspect {
    pub service: String,
    pub score: f64,
    pub direct_error: f64,
    pub direct_anomaly: f64,
    pub propagated: f64,
    pub direct_latency: f64,
    pub temporal_mult: f64,
    pub contributors: Vec<(String, String, f64)>,
}
