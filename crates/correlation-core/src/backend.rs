use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub type TraceId = String;
pub type SpanId  = String;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Span {
    pub span_id: SpanId,
    pub trace_id: TraceId,
    pub parent_id: Option<SpanId>,
    pub service: String,
    pub operation: String,
    pub start: DateTime<Utc>,
    pub duration_ms: i64,
    pub status: SpanStatus,
    pub status_message: Option<String>,
    pub attributes: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SpanStatus { Ok, Error }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogRecord {
    pub ts: DateTime<Utc>,
    pub service: String,
    pub level: String,
    pub message: String,
    pub trace_id: Option<TraceId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogQuery {
    pub services: Vec<String>,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub level_at_least: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricQuery {
    pub metric: String,
    pub service: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyWindowQuery {
    pub metric: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricPoint { pub ts: DateTime<Utc>, pub service: String, pub value: f64 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeSeries { pub service: String, pub metric: String, pub points: Vec<(DateTime<Utc>, f64)> }

#[derive(Debug, Error)]
pub enum BackendError {
    #[error("unreachable")]                                  Unreachable,
    #[error("timeout")]                                       Timeout,
    #[error("partial content: {0}")]                          PartialContent(String),
    #[error("malformed response")]                            MalformedResponse,
    #[error("rate limited")]                                  RateLimited,
    #[error("retention miss before {0}")]                     RetentionMiss(DateTime<Utc>),
    #[error("empty")]                                          Empty,
}

#[async_trait]
pub trait TelemetryBackend: Send + Sync {
    async fn fetch_trace(&self, id: TraceId) -> Result<Vec<Span>, BackendError>;
    async fn fetch_logs(&self, q: LogQuery) -> Result<Vec<LogRecord>, BackendError>;
    async fn fetch_metric_series(&self, q: MetricQuery) -> Result<Vec<TimeSeries>, BackendError>;
    async fn query_metric_window(&self, q: AnomalyWindowQuery) -> Result<Vec<MetricPoint>, BackendError>;
}

pub struct RetryPolicy { pub attempts: u32, pub backoffs_ms: Vec<u64> }
impl Default for RetryPolicy { fn default() -> Self { Self { attempts: 3, backoffs_ms: vec![100, 400, 1600] } } }
impl RetryPolicy {
    pub async fn run<F, Fut, T>(&self, mut f: F) -> anyhow::Result<T>
    where F: FnMut() -> Fut, Fut: std::future::Future<Output = anyhow::Result<T>> {
        let mut last_err: Option<anyhow::Error> = None;
        for i in 0..self.attempts {
            match f().await { Ok(v) => return Ok(v), Err(e) => { last_err = Some(e); } }
            if i + 1 < self.attempts {
                tokio::time::sleep(std::time::Duration::from_millis(self.backoffs_ms.get(i as usize).copied().unwrap_or(1000))).await;
            }
        }
        Err(last_err.unwrap())
    }
}
