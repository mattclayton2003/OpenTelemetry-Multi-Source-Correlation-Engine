use async_trait::async_trait;
use correlation_core::backend::*;

pub struct LokiClient {
    pub base_url: String,
    pub http: reqwest::Client,
    pub retry: RetryPolicy,
}
impl LokiClient {
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            http: reqwest::Client::new(),
            retry: RetryPolicy::default(),
        }
    }
}

#[async_trait]
impl TelemetryBackend for LokiClient {
    async fn fetch_trace(&self, _id: TraceId) -> Result<Vec<Span>, BackendError> {
        Ok(vec![])
    }
    async fn fetch_logs(&self, q: LogQuery) -> Result<Vec<LogRecord>, BackendError> {
        if q.services.is_empty() {
            return Ok(vec![]);
        }
        let svc_or = q
            .services
            .iter()
            .map(|s| format!("service_name=\"{s}\""))
            .collect::<Vec<_>>()
            .join("|");
        let logql = format!("{{{svc_or}}}");
        let start = q.start.timestamp_nanos_opt().unwrap_or(0).to_string();
        let end = q.end.timestamp_nanos_opt().unwrap_or(0).to_string();
        let url = format!("{}/loki/api/v1/query_range", self.base_url);
        let v: serde_json::Value = self
            .retry
            .run(|| {
                let url = url.clone();
                let http = self.http.clone();
                let logql = logql.clone();
                let s = start.clone();
                let e = end.clone();
                async move {
                    let r = http
                        .get(&url)
                        .query(&[
                            ("query", logql.as_str()),
                            ("start", s.as_str()),
                            ("end", e.as_str()),
                            ("limit", "5000"),
                        ])
                        .send()
                        .await?;
                    if !r.status().is_success() {
                        return Err(anyhow::anyhow!("status {}", r.status()));
                    }
                    Ok(r.json::<serde_json::Value>().await?)
                }
            })
            .await
            .map_err(|_| BackendError::Unreachable)?;
        parse_loki(v)
    }
    async fn fetch_metric_series(&self, _q: MetricQuery) -> Result<Vec<TimeSeries>, BackendError> {
        Ok(vec![])
    }
    async fn query_metric_window(
        &self,
        _q: AnomalyWindowQuery,
    ) -> Result<Vec<MetricPoint>, BackendError> {
        Ok(vec![])
    }
}

fn parse_loki(v: serde_json::Value) -> Result<Vec<LogRecord>, BackendError> {
    use chrono::{TimeZone, Utc};
    let mut out = vec![];
    let result = v["data"]["result"]
        .as_array()
        .ok_or(BackendError::MalformedResponse)?;
    for stream in result {
        let svc = stream["stream"]["service_name"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();
        let level = stream["stream"]["level"]
            .as_str()
            .unwrap_or("INFO")
            .to_string();
        for entry in stream["values"].as_array().unwrap_or(&vec![]) {
            let arr = entry.as_array().ok_or(BackendError::MalformedResponse)?;
            let ts_ns: i64 = arr[0].as_str().unwrap_or("0").parse().unwrap_or(0);
            let msg = arr[1].as_str().unwrap_or("").to_string();
            out.push(LogRecord {
                ts: Utc.timestamp_nanos(ts_ns),
                service: svc.clone(),
                level: level.clone(),
                message: msg,
                trace_id: None,
            });
        }
    }
    Ok(out)
}
