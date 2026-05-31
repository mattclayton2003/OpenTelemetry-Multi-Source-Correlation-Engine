use crate::backend::{SpanStatus};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub type NodeId = String;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Node {
    Service { name: String },
    Span    { id: String, service: String, operation: String, status: SpanStatus,
              start: DateTime<Utc>, duration_ms: i64, parent: Option<String>,
              status_message: Option<String> },
    LogBatch { id: String, service: String, level: String, bucket_start: DateTime<Utc>,
               count: usize, samples: Vec<String> },
    MetricAnomaly { id: String, service: String, metric: String,
                    window_start: DateTime<Utc>, window_end: DateTime<Utc>,
                    severity: f64, detector: String,
                    baseline_mean: f64, observed_peak: f64 },
}

impl Node {
    pub fn service(name: String) -> Self { Node::Service { name } }
    pub fn id(&self) -> NodeId {
        match self {
            Node::Service { name } => format!("svc:{name}"),
            Node::Span { id, .. } => format!("span:{id}"),
            Node::LogBatch { id, .. } => format!("lb:{id}"),
            Node::MetricAnomaly { id, .. } => format!("anom:{id}"),
        }
    }
    pub fn service_name(&self) -> Option<&str> {
        match self {
            Node::Service { name } => Some(name),
            Node::Span { service, .. } | Node::LogBatch { service, .. } | Node::MetricAnomaly { service, .. } => Some(service),
        }
    }
}
