use rand::Rng;

pub struct FailureModes {
    pub latency_ms_env: Option<u64>,
    pub error_rate_env: Option<f64>,
    pub cold_start_first_n: Option<u64>,
}

impl FailureModes {
    pub fn from_env(prefix: &str) -> Self {
        let g = |k: &str| std::env::var(format!("{prefix}_INJECT_{k}")).ok();
        Self {
            latency_ms_env: g("LATENCY_MS").and_then(|v| v.parse().ok()),
            error_rate_env: g("ERROR_RATE").and_then(|v| v.parse().ok()),
            cold_start_first_n: g("COLD_START_FIRST_N").and_then(|v| v.parse().ok()),
        }
    }
    pub fn latency_ms(&self) -> Option<u64> { self.latency_ms_env }
    pub fn should_inject_error(&self) -> bool {
        match self.error_rate_env {
            Some(r) if r > 0.0 => rand::thread_rng().gen::<f64>() < r,
            _ => false,
        }
    }
    pub async fn maybe_delay(&self) {
        if let Some(ms) = self.latency_ms_env {
            tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
        }
    }
}
