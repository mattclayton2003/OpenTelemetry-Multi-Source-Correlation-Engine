pub mod renderer_md;
pub mod version;
pub use version::SCHEMA_VERSION;

use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncidentContext {
    pub schema_version: String,
    pub incident_id: String,
    pub produced_at: DateTime<Utc>,
    pub engine_version: String,
    pub config_hash: String,
    pub elapsed_ms: u64,
    pub trigger: Trigger,
    pub window: Window,
    pub services: Vec<ServiceSummary>,
    pub suspects: Vec<Suspect>,
    pub spans: Vec<SpanRef>,
    pub span_tree: Vec<TreeNode>,
    pub log_batches: Vec<LogBatchRef>,
    pub metric_anomalies: Vec<MetricAnomalyRef>,
    pub timeline: Vec<TimelineEvent>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Trigger {
    Trace { trace: TraceTrigger },
    Anomaly { anomaly: AnomalyTrigger },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceTrigger {
    pub trace_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyTrigger {
    pub metric: String,
    pub service: String,
    pub window: Window,
    pub observed_value: f64,
    pub baseline_mean: f64,
    pub baseline_stddev: f64,
    pub z_score: f64,
    pub detector: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Window {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    #[serde(default)]
    pub expanded: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceSummary {
    pub name: String,
    pub span_count: usize,
    pub error_span_count: usize,
    pub log_count: usize,
    pub error_log_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Suspect {
    pub rank: usize,
    pub service: String,
    pub score: f64,
    pub evidence_breakdown: EvidenceBreakdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceBreakdown {
    pub direct_error_weight: f64,
    pub direct_anomaly_weight: f64,
    pub propagated_weight: f64,
    pub temporal_tightness_multiplier: f64,
    pub contributors: Vec<Contributor>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contributor {
    pub kind: String,
    pub r#ref: String,
    pub weight: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanRef {
    pub id: String,
    pub trace_id: String,
    pub parent_id: Option<String>,
    pub service: String,
    pub operation: String,
    pub start: DateTime<Utc>,
    pub duration_ms: i64,
    pub status: String,
    pub status_message: Option<String>,
    pub attributes: IndexMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeNode {
    pub span_id: String,
    pub children: Vec<TreeNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogBatchRef {
    pub id: String,
    pub service: String,
    pub level: String,
    pub time_bucket: String,
    pub count: usize,
    pub sample_messages: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricAnomalyRef {
    pub id: String,
    pub service: String,
    pub metric: String,
    pub window: Window,
    pub severity: f64,
    pub detector: String,
    pub baseline_mean: f64,
    pub observed_peak: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEvent {
    pub ts: DateTime<Utc>,
    pub kind: String,
    pub r#ref: String,
}
