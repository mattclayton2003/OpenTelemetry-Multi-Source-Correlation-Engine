use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
pub struct CoverageTargets {
    #[serde(flatten)] pub classes: HashMap<String, ClassEntry>,
}
#[derive(Debug, Deserialize)]
pub struct ClassEntry {
    pub metrics: Vec<String>,
}

impl CoverageTargets {
    pub fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        Ok(toml::from_str(&std::fs::read_to_string(path)?)?)
    }
    pub fn expected_for(&self, class: &str) -> Vec<String> {
        self.classes.get(class).map(|e| e.metrics.clone()).unwrap_or_default()
    }
}

#[derive(Debug, Deserialize)]
pub struct AnomalyInvocation {
    #[serde(flatten)] pub classes: HashMap<String, InvocationEntry>,
}
#[derive(Debug, Deserialize)]
pub struct InvocationEntry {
    pub metric: String, pub service: String,
    pub window_pre_sec: i64, pub window_post_sec: i64,
}
impl AnomalyInvocation {
    pub fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        Ok(toml::from_str(&std::fs::read_to_string(path)?)?)
    }
}

use correlation_core::IncidentContext;

pub fn trace_coverage(ic: &IncidentContext, denominator_from_tempo: usize) -> f64 {
    let denom = denominator_from_tempo.max(1) as f64;
    let trace_ids: std::collections::HashSet<&str> =
        ic.spans.iter().map(|s| s.trace_id.as_str()).collect();
    (trace_ids.len() as f64 / denom).min(1.0)
}

pub fn error_log_coverage(ic: &IncidentContext, denom: usize) -> f64 {
    let denom = denom.max(1) as f64;
    let err_logs = ic.log_batches.iter().filter(|b| b.level == "ERROR").map(|b| b.count).sum::<usize>();
    (err_logs as f64 / denom).min(1.0)
}

pub fn anomaly_coverage(ic: &IncidentContext, expected_metrics: &[String]) -> f64 {
    if expected_metrics.is_empty() { return 1.0; }
    let present: std::collections::HashSet<&str> =
        ic.metric_anomalies.iter().map(|a| a.metric.as_str()).collect();
    let hit = expected_metrics.iter().filter(|m| present.contains(m.as_str())).count() as f64;
    hit / expected_metrics.len() as f64
}

pub fn tree_integrity(ic: &IncidentContext) -> f64 {
    let known: std::collections::HashSet<&str> = ic.spans.iter().map(|s| s.id.as_str()).collect();
    let mut total = 0usize; let mut ok = 0usize;
    for sp in &ic.spans {
        total += 1;
        if let Some(p) = &sp.parent_id {
            if known.contains(p.as_str()) { ok += 1; }
        } else { ok += 1; }
    }
    if total == 0 { 1.0 } else { ok as f64 / total as f64 }
}
