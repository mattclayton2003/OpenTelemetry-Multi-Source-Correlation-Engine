use correlation_core::backend::*;
use async_trait::async_trait;

pub struct TempoClient { pub base_url: String, pub http: reqwest::Client, pub retry: RetryPolicy }

impl TempoClient {
    pub fn new(base_url: String) -> Self {
        Self { base_url, http: reqwest::Client::new(), retry: RetryPolicy::default() }
    }
}

#[async_trait]
impl TelemetryBackend for TempoClient {
    async fn fetch_trace(&self, id: TraceId) -> Result<Vec<Span>, BackendError> {
        let url = format!("{}/api/traces/{id}", self.base_url);
        let v: serde_json::Value = self.retry.run(|| {
            let url = url.clone(); let http = self.http.clone();
            async move {
                let r = http.get(&url).send().await?;
                if r.status() == 404 { return Err(anyhow::anyhow!("not found (404)")); }
                if !r.status().is_success() { return Err(anyhow::anyhow!("status {}", r.status())); }
                Ok(r.json::<serde_json::Value>().await?)
            }
        }).await.map_err(|e| {
            if e.to_string().contains("404") { BackendError::Empty }
            else { BackendError::Unreachable }
        })?;
        parse_tempo_trace(v)
    }
    async fn fetch_logs(&self, _q: LogQuery) -> Result<Vec<LogRecord>, BackendError> { Ok(vec![]) }
    async fn fetch_metric_series(&self, _q: MetricQuery) -> Result<Vec<TimeSeries>, BackendError> { Ok(vec![]) }
    async fn query_metric_window(&self, _q: AnomalyWindowQuery) -> Result<Vec<MetricPoint>, BackendError> { Ok(vec![]) }
}

fn parse_tempo_trace(v: serde_json::Value) -> Result<Vec<Span>, BackendError> {
    use chrono::{Utc, TimeZone};
    let mut out = vec![];
    let batches = v["batches"].as_array().ok_or(BackendError::MalformedResponse)?;
    for batch in batches {
        let service = batch["resource"]["attributes"].as_array().and_then(|attrs| {
            attrs.iter().find(|a| a["key"] == "service.name")
                .and_then(|a| a["value"]["stringValue"].as_str().map(|s| s.to_string()))
        }).unwrap_or_else(|| "unknown".into());
        for ss in batch["scopeSpans"].as_array().unwrap_or(&vec![]) {
            for sp in ss["spans"].as_array().unwrap_or(&vec![]) {
                let start_ns: i64 = sp["startTimeUnixNano"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0);
                let end_ns:   i64 = sp["endTimeUnixNano"].as_str().and_then(|s| s.parse().ok()).unwrap_or(start_ns);
                let dur_ms = ((end_ns - start_ns) / 1_000_000).max(0);
                let status_code = sp["status"]["code"].as_i64().unwrap_or(0);
                out.push(Span {
                    span_id:  sp["spanId"].as_str().unwrap_or("").into(),
                    trace_id: sp["traceId"].as_str().unwrap_or("").into(),
                    parent_id: sp["parentSpanId"].as_str().filter(|s| !s.is_empty()).map(|s| s.into()),
                    service:  service.clone(),
                    operation: sp["name"].as_str().unwrap_or("").into(),
                    start: Utc.timestamp_nanos(start_ns),
                    duration_ms: dur_ms,
                    status: if status_code == 2 { SpanStatus::Error } else { SpanStatus::Ok },
                    status_message: sp["status"]["message"].as_str().map(|s| s.into()),
                    attributes: serde_json::Map::new(),
                });
            }
        }
    }
    Ok(out)
}
